// Every paired machine's connection and what it last said: online or not, what this
// device may do, and its conversations. Screens read it with `useHub`; it reconnects on
// its own (1 s, doubling, at most 30 s) except when the machine refused this device; what
// each close code means is in `close.ts`. Cards of a machine that is not online are what
// it last said, not what is true now.
import { useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { tk, tr } from '../i18n';
import { forgetMachine, loadMachines, machineId, saveMachine } from '../store/machines';
import { forgetQuota } from '../store/quota';
import { type CardsReply, patch, refreshWait } from './cards';
import { HEARTBEAT_MS, Remote, type RemoteError } from './client';
import { named } from './named';
import { closeOutcome } from './close';
import type { Link } from './link';
import type { Space } from '../ui/spaces';

export type Phase = 'working' | 'waiting' | 'idle' | 'shell';

/** A terminal as `list` gives it. */
export type AgentControls = { steer: boolean; send_now: boolean; effort: boolean; suggestion?: boolean; busy_input?: 'queue' | 'steer' | null };

export type Card = {
  agent_name?: string;
  controls?: AgentControls | null;
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
  /** The board column its host put it in (needs, running, review, done, stale); absent
   *  for shells and conversations off the board. */
  column?: string | null;
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
  permission_full?: boolean | null;
};

/** A todo you have not closed. Hosts before 11-08 send the first four fields only, and only
 *  todos nobody started. */
export type Todo = {
  agent_name?: string;
  id: string;
  space: string;
  title: string;
  agent: string;
  /** `TaskState::key` on the computer: not_started, working, settled, proven, deferred. */
  state?: string | null;
  /** Its words for that state, shown as they are. */
  state_label?: string | null;
  /** What you wrote when you noted it. */
  said?: string;
  acceptance?: string[];
  /** Progress written back, oldest first. */
  notes?: { at: number; text: string }[];
  created?: number | null;
  /** The open terminal of a conversation working on it. */
  terminal?: string | null;
};

export type Machine = {
  agentNames?: Record<string, string>;
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
  /** The spaces in the computer's order, the ones put aside left out: where a new task can go. */
  spaceList?: Space[];
  /** Spaces put aside on the computer (先不管): their conversations do not come here. */
  quietSpaces?: Space[];
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
  // A clean hub for whoever stages it: test files share this module in one process.
  state = { ...state, loaded: false, machines: [] };
  remotes.clear();
  dialing.clear();
  flights.clear();
  for (const id of [...retry.keys()]) stopRetry(id);
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

/** What `spaces` says, as a machine keeps it; only when the reply says it. */
function listed(list: { spaces?: any[]; quiet?: any[] | null; todos?: Todo[]; machine?: { name?: string; agent_names?: Record<string, string> } }, id: string): Partial<Machine> {
  return {
    ...(list.spaces && {
      spaces: Object.fromEntries(list.spaces.map((s: any) => [s.id, s.name])),
      spaceDirs: Object.fromEntries(list.spaces.filter((s: any) => s.cwd).map((s: any) => [s.cwd, s.name])),
      spaceList: list.spaces.map((s: any) => ({ id: String(s.id), name: String(s.name ?? ''), cwd: String(s.cwd ?? '') })),
    }),
    ...(Array.isArray(list.quiet) && { quietSpaces: list.quiet.map((s: any) => ({ id: String(s.id), name: String(s.name ?? ''), cwd: String(s.cwd ?? '') })) }),
    ...(list.todos && { todos: list.todos }),
    name: list.machine?.name || get(id)?.name || '',
    ...(list.machine && { agentNames: list.machine.agent_names ?? {} }),
    updated: Date.now() / 1000,
  };
}

/** The `rev` each connection last gave, what its next `cards` is relative to. */
const revs = new WeakMap<Remote, number>();

/** The list from the machine: as what changed when it can say that, else whole. */
async function load(id: string, r: Remote) {
  if (!supports(get(id), 'cards')) {
    const list = await r.request('list');
    set(id, { cards: list.terminals, ...listed({ ...list, spaces: list.spaces ?? [], todos: list.todos ?? [] }, id) });
    return;
  }
  const since = revs.get(r);
  const reply: CardsReply<Card> & Parameters<typeof listed>[0] = await r.request('cards', since === undefined ? {} : { since });
  if (remotes.get(id) !== r) return;
  const cards = patch(since === undefined ? [] : (get(id)?.cards ?? []), reply);
  if (!cards) {
    // It named a card this phone does not have: out of step, so the whole list.
    revs.delete(r);
    return load(id, r);
  }
  revs.set(r, reply.rev);
  set(id, { cards, ...listed(reply, id) });
}

/** The refresh on its way for a machine, and the one that follows it if more were asked for. */
const flights = new Map<string, { done: Promise<void>; again?: Promise<void> }>();

/** Reads the list now, or once more after the read in flight: their answers would race. */
function refresh(id: string): Promise<void> {
  const flying = flights.get(id);
  if (flying) return (flying.again ??= flying.done.then(() => refresh(id)));
  const r = remotes.get(id);
  if (!r) return Promise.resolve();
  const flight: { done: Promise<void>; again?: Promise<void> } = {
    done: load(id, r)
      // A failed list on a live connection shows as the connection ending.
      .catch(() => undefined)
      .finally(() => {
        if (flights.get(id) === flight) flights.delete(id);
      }),
  };
  flights.set(id, flight);
  return flight.done;
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
  // What the machine answered when it turned this phone away after the handshake.
  let refusal: string | undefined;
  r.onEnded((e) => {
    // Only the current connection's end says anything about the machine.
    if (remotes.get(id) !== r) return;
    remotes.delete(id);
    ended(id, refusal ? undefined : e.code, refusal, e.reason);
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
      agentNames: hello.machine?.agent_names ?? {},
      os: typeof hello.os === 'string' ? hello.os : undefined,
      appVersion: typeof hello.app_version === 'string' ? hello.app_version : undefined,
      seen: Date.now() / 1000,
    });
    ping(id, r);
    await follow(id, r);
    await refresh(id);
  } catch (e) {
    // The connection still up means the machine itself said no: say what it said, not
    // that the network is down.
    if (!r.ended) refusal = tr('电脑上的 Fibo 没接受这台手机：{why}', { why: (e as Error).message });
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
  let last = 0;
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
      last = Date.now();
      refresh(id);
    }, refreshWait(Date.now(), last));
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

/** What a screen says instead of its data when the machine lacks the op; a key: show it as `tr(NOT_YET)`. */
export const NOT_YET = tk('这台电脑的 Fibo 还不支持，更新后可用');

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
