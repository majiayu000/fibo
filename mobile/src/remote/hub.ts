// Every paired machine's connection and what it last said: online or not, what this
// device may do, and its conversations. Screens read it with `useHub`; it reconnects on
// its own (1 s, doubling, at most 30 s) except when the machine refused this device; what
// each close code means is in `close.ts`. Cards of a machine that is not online are what
// it last said, not what is true now.
import { useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { forgetMachine, loadMachines, machineId, saveMachine } from '../store/machines';
import { Remote, type RemoteError } from './client';
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
};

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
  /** Seconds since the epoch of the last list. */
  updated?: number;
};

type State = { loaded: boolean; machines: Machine[] };

let state: State = { loaded: false, machines: [] };
const listeners = new Set<() => void>();
const remotes = new Map<string, Remote>();
const retry = new Map<string, { delay: number; timer?: ReturnType<typeof setTimeout> }>();

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
      name: list.machine?.name || get(id)?.name || '',
      updated: Date.now() / 1000,
    });
  } catch {
    // A failed list on a live connection shows as the connection ending.
  }
}

async function connect(id: string) {
  const m = get(id);
  if (!m || remotes.has(id)) return;
  const pending = retry.get(id);
  if (pending?.timer) clearTimeout(pending.timer);
  set(id, { status: 'connecting', error: undefined });
  let r: Remote;
  try {
    r = await Remote.connect(m.link);
  } catch (e) {
    ended(id, (e as RemoteError).code, (e as Error).message);
    return;
  }
  remotes.set(id, r);
  r.onEnded((e) => {
    remotes.delete(id);
    ended(id, e.code);
  });
  try {
    const hello = await r.request('hello', { version: 1 });
    retry.delete(id);
    set(id, {
      status: 'online',
      role: hello.role,
      ops: hello.ops ?? [],
      name: hello.machine?.name || m.name,
    });
    await follow(id, r);
    await refresh(id);
  } catch (e) {
    r.close();
  }
}

/** The connection is down: shown by its close code, and tried again unless refused. */
function ended(id: string, code: number | undefined, fallback?: string) {
  if (!get(id)) return;
  const o = closeOutcome(code);
  // Without a code, what the connection said (a timeout, a bad handshake) says more.
  set(id, { status: o.status, error: code === undefined && fallback ? fallback : o.text });
  if (o.retry === 'now') {
    retry.delete(id);
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
  r.timer = setTimeout(() => connect(id), r.delay);
  r.delay = Math.min(r.delay * 2, 30_000);
  retry.set(id, r);
}

export async function start() {
  if (state.loaded) return;
  const links = await loadMachines();
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
        retry.delete(m.id);
        connect(m.id);
      } else if (m.status === 'online') refresh(m.id);
    }
  });
}

export async function pair(link: Link) {
  await saveMachine(link);
  const id = machineId(link);
  remotes.get(id)?.close();
  remotes.delete(id);
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
  remotes.get(id)?.close();
  remotes.delete(id);
  const r = retry.get(id);
  if (r?.timer) clearTimeout(r.timer);
  retry.delete(id);
  await forgetMachine(id);
  state = { ...state, machines: state.machines.filter((m) => m.id !== id) };
  listeners.forEach((l) => l());
}

export function reconnect(id: string) {
  retry.delete(id);
  connect(id);
}

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
