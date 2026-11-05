// Every paired machine's connection and what it last said: online or not, what this
// device may do, and its conversations. Screens read it with `useHub`; it reconnects on
// its own (1 s, doubling, at most 30 s) except when the machine refused this device; what
// each close code means is in `close.ts`. Cards of a machine that is not online are what
// it last said, not what is true now.
import { useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { forgetMachine, loadMachines, machineId, saveMachine } from '../store/machines';
import { forgetQuota } from '../store/quota';
import { HEARTBEAT_MS, Remote, type RemoteError } from './client';
import { named } from './named';
import { closeOutcome } from './close';
import type { Link } from './link';

export type Phase = 'working' | 'waiting' | 'idle' | 'shell';

/** A terminal as `list` gives it. */
export type Card = {
  id: string;
  space: string;
  tab: string;
  cwd: string;
  phase: Phase;
  agent?: string | null;
  session?: string | null;
  title?: string | null;
  approval?: string | null;
  waiting?: string | null;
  /** The desktop's state (`Status::key`) and its word; absent from hosts before 11-03. */
  status?: string | null;
  status_label?: string | null;
  /** Waiting on you or failed, counted as the desktop counts 要你处理. */
  needs_you?: boolean;
  /** It asks to be trusted with this folder before anything else; `status` says it waits. */
  trust?: { question: string; folder: string } | null;
  /** An Agent runs but no conversation is tied to it, in the user's words. */
  note?: string | null;
  /** The conversation's current turn, for `outcome`. */
  turn?: string | null;
  /** open, done, deferred or abandoned. */
  outcome?: string | null;
  /** When the conversation last changed, Unix seconds. */
  updated?: number | null;
  /** The branch checked out in its directory. */
  branch?: string | null;
  /** When the turn began, Unix seconds, while its state is running. */
  running_since?: number | null;
  /** While running, what its log says it does now: 「运行 cargo test」. */
  doing?: string | null;
  /** The model that last answered, and its permission mode in words to show. */
  model?: string | null;
  permission?: string | null;
};

/** A todo no conversation works on yet, the board's 待办. */
export type Todo = { id: string; space: string; title: string; agent: string };

export type Machine = {
  id: string;
  link: Link;
  name: string;
  status: 'connecting' | 'online' | 'offline' | 'refused';
  /** Why it is offline or refused, as people read it. */
  error?: string;
  role?: 'view' | 'answer' | 'full';
  ops: string[];
  cards: Card[];
  /** Space id to name. */
  spaces: Record<string, string>;
  /** A space's directory to its name. */
  spaceDirs?: Record<string, string>;
  todos?: Todo[];
  /** Seconds since the epoch of the last list. */
  updated?: number;
  /** `macos` or `linux`; from hosts that speak the 11-04 contract (v2-plan.md). */
  os?: string;
  /** The host's app version, when it says. */
  appVersion?: string;
  /** Round trip of the last `ping`, measured here, in ms. */
  latency?: number;
  /** Seconds since the epoch when it was last online. */
  seen?: number;
};

type State = { loaded: boolean; machines: Machine[] };

let state: State = { loaded: false, machines: [] };

type Dial = (link: Link) => Promise<Remote>;
let dial: Dial = (link) => Remote.connect(link);
let staged: Link[] | null = null;

/**
 * Screenshots only (`EXPO_PUBLIC_FIXTURE=1`, see `src/fixture.ts`): these machines instead
 * of the paired ones, reached through `d`. Never called in a normal build.
 */
export function stage(links: Link[], d: Dial) {
  staged = links;
  dial = d;
}
const listeners = new Set<() => void>();
const remotes = new Map<string, Remote>();
const retry = new Map<string, { delay: number; timer?: ReturnType<typeof setTimeout> }>();
/** The dial in flight for a machine: one at a time, and one that is no longer wanted closes. */
const dialing = new Map<string, symbol>();

/** Stops any waiting retry, and starts the backoff over. */
function stopRetry(id: string) {
  const r = retry.get(id);
  if (r?.timer) clearTimeout(r.timer);
  retry.delete(id);
}

function set(id: string, patch: Partial<Machine>) {
  state = {
    ...state,
    machines: state.machines.map((m) => (m.id === id ? { ...m, ...patch } : m)),
  };
  listeners.forEach((l) => l());
}

const get = (id: string) => state.machines.find((m) => m.id === id);

async function refresh(id: string) {
  const r = remotes.get(id);
  if (!r) return;
  try {
    const list = await r.request('list');
    set(id, {
      cards: list.terminals,
      spaces: Object.fromEntries((list.spaces ?? []).map((s: any) => [s.id, s.name])),
      spaceDirs: Object.fromEntries((list.spaces ?? []).filter((s: any) => s.cwd).map((s: any) => [s.cwd, s.name])),
      todos: list.todos ?? [],
      name: list.machine?.name || get(id)?.name || '',
      updated: Date.now() / 1000,
    });
  } catch {
    // A failed list on a live connection shows as the connection ending.
  }
}

async function connect(id: string) {
  const m = get(id);
  // A machine has one connection: every caller (a timer, coming to the foreground, 重试) gets
  // the one there is, or the one on its way.
  if (!m || remotes.has(id) || dialing.has(id)) return;
  const pending = retry.get(id);
  if (pending?.timer) clearTimeout(pending.timer);
  if (pending) pending.timer = undefined;
  const turn = Symbol();
  dialing.set(id, turn);
  set(id, { status: 'connecting', error: undefined });
  let r: Remote;
  try {
    r = await dial(m.link);
  } catch (e) {
    if (dialing.get(id) !== turn) return;
    dialing.delete(id);
    ended(id, (e as RemoteError).code, (e as Error).message, (e as RemoteError).reason);
    return;
  }
  // Removed or paired again while dialing: this one is nobody's.
  if (dialing.get(id) !== turn) {
    r.close();
    return;
  }
  dialing.delete(id);
  remotes.set(id, r);
  r.onEnded((e) => {
    // Only the current connection's end says anything about the machine.
    if (remotes.get(id) !== r) return;
    remotes.delete(id);
    ended(id, e.code, undefined, e.reason);
  });
  try {
    // The device id names this phone for the request ids its writes carry.
    const hello = await r.request('hello', { version: 1, client: m.link.device });
    if (remotes.get(id) !== r) return;
    const beat = setInterval(() => {
      if (r.ended) clearInterval(beat);
      else ping(id, r);
    }, HEARTBEAT_MS);
    r.onEnded(() => clearInterval(beat));
    retry.delete(id);
    set(id, {
      status: 'online',
      role: hello.role,
      ops: hello.ops ?? [],
      name: hello.machine?.name || m.name,
      os: typeof hello.os === 'string' ? hello.os : undefined,
      appVersion: typeof hello.app_version === 'string' ? hello.app_version : undefined,
      seen: Date.now() / 1000,
    });
    ping(id, r);
    await follow(id, r);
    await refresh(id);
  } catch (e) {
    r.close();
  }
}

/** The heartbeat, timed: the phone's own measure of the link's latency. */
function ping(id: string, r: Remote) {
  const sent = Date.now();
  r.request('ping')
    .then(() => {
      if (remotes.get(id) === r) set(id, { latency: Date.now() - sent, seen: Date.now() / 1000 });
    })
    .catch(() => undefined);
}

/** The connection is down: shown by its close code, and tried again unless refused. */
function ended(id: string, code: number | undefined, fallback?: string, reason?: string) {
  if (!get(id)) return;
  const o = closeOutcome(code, reason);
  // Without a code, what the connection said (a timeout, a bad handshake) says more.
  set(id, { status: o.status, error: code === undefined && fallback ? fallback : o.text });
  if (o.retry === 'now') {
    stopRetry(id);
    connect(id);
  } else if (o.retry === 'later') later(id);
}

/** Follows the machine's events, a burst of them one list; opened again if it ends alone. */
function follow(id: string, r: Remote): Promise<unknown> {
  let queued = false;
  return r.stream('events', {}, (m) => {
    if (m.type === 'end') {
      // The connection ending ends it too; then the reconnect opens a new one.
      if (r.ended || remotes.get(id) !== r) return;
      setTimeout(() => {
        if (!r.ended && remotes.get(id) === r) follow(id, r).catch(() => r.close());
      }, 1000);
      return;
    }
    if (queued) return;
    queued = true;
    setTimeout(() => {
      queued = false;
      refresh(id);
    }, 150);
  });
}

function later(id: string) {
  const r = retry.get(id) ?? { delay: 1000 };
  if (r.timer) clearTimeout(r.timer);
  r.timer = setTimeout(() => connect(id), r.delay);
  r.delay = Math.min(r.delay * 2, 30_000);
  retry.set(id, r);
}

export async function start() {
  if (state.loaded) return;
  const links = staged ?? (await loadMachines());
  state = {
    loaded: true,
    machines: links.map((link) => ({
      id: machineId(link),
      link,
      name: link.name,
      status: 'offline',
      ops: [],
      cards: [],
      spaces: {},
    })),
  };
  listeners.forEach((l) => l());
  state.machines.forEach((m) => connect(m.id));
  AppState.addEventListener('change', (s) => {
    if (s !== 'active') return;
    // Back from the background: anything that dropped meanwhile tries again now.
    for (const m of state.machines) {
      if (m.status === 'offline') {
        stopRetry(m.id);
        connect(m.id);
      } else if (m.status === 'online') refresh(m.id);
    }
  });
}

/** Pairs with `link`: kept only once its machine answered this phone's handshake. */
export async function pair(link: Link) {
  const probe = await dial(link);
  probe.close();
  await saveMachine(link);
  const id = machineId(link);
  const old = remotes.get(id);
  remotes.delete(id);
  old?.close();
  dialing.delete(id);
  stopRetry(id);
  const fresh: Machine = { id, link, name: link.name, status: 'offline', ops: [], cards: [], spaces: {} };
  state = {
    ...state,
    machines: get(id)
      ? state.machines.map((m) => (m.id === id ? fresh : m))
      : [...state.machines, fresh],
  };
  listeners.forEach((l) => l());
  await connect(id);
}

export async function forget(id: string) {
  const old = remotes.get(id);
  remotes.delete(id);
  old?.close();
  dialing.delete(id);
  stopRetry(id);
  await forgetMachine(id);
  await forgetQuota(id);
  state = { ...state, machines: state.machines.filter((m) => m.id !== id) };
  listeners.forEach((l) => l());
}

export function reconnect(id: string) {
  stopRetry(id);
  connect(id);
}

/** Whether the machine offers `op` to this phone (its `hello` lists it). */
export const supports = (m: Machine | undefined, op: string) => !!m && m.status === 'online' && m.ops.includes(op);

/** Fields of the 11-04 contract on old ops (answer `always`, chat `tools`, open `agent`):
 * a host that says its `os` in hello speaks it; an older one would drop them silently. */
export const speaksV2 = (m: Machine | undefined) => !!m?.os;

/** What a screen says instead of its data when the machine lacks the op. */
export const NOT_YET = '这台电脑的 Mooring 还不支持，更新后可用';

export const getMachine = get;

/** The live connection to a machine, for one-off requests; `null` while it is down. */
export const remote = (id: string) => remotes.get(id) ?? null;

export { refresh };

export function useHub(): State {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => state,
  );
}

/** A write named by a request id, sent again under it after a reconnect (`named.ts`). */
export function write(machine: string, op: string, fields: Record<string, unknown>) {
  return named(() => remotes.get(machine), op, fields);
}
