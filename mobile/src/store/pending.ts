// Messages sent from this phone that the conversation's log does not have yet, kept per
// terminal. A deep link to another conversation reuses the chat screen, so state held in it
// followed along: Claude's 「已立刻发送」 bubble showed under a Codex conversation (2026-11-18 walk).
import { useSyncExternalStore } from 'react';
import type { How } from '@/ui/busy';

/** `queued`: sent while the Agent worked; what that means depends on the CLI (ui/busy.ts). */
export type Pending = { text: string; at: number; queued?: boolean; pictures?: number; how?: How; now?: boolean };

const NONE: Pending[] = [];
let all = new Map<string, Pending[]>();
const listeners = new Set<() => void>();
const keyOf = (machine: string, terminal: string) => `${machine}\n${terminal}`;

/** What the terminal's conversation has on its way. */
export const pendingFor = (machine: string, terminal: string) => all.get(keyOf(machine, terminal)) ?? NONE;

/** Changes one terminal's list; the others stay as they are. */
export function updatePending(machine: string, terminal: string, change: (p: Pending[]) => Pending[]) {
  const key = keyOf(machine, terminal);
  const had = all.get(key) ?? NONE;
  const next = change(had);
  if (next === had || (!next.length && !had.length)) return;
  all = new Map(all);
  if (next.length) all.set(key, next);
  else all.delete(key);
  listeners.forEach((l) => l());
}

export const usePending = (machine: string, terminal: string) =>
  useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => pendingFor(machine, terminal),
  );
