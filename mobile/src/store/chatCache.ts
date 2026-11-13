// The last messages of each conversation seen, kept on this phone: opened while the computer
// is away, a conversation shows what it said last time instead of nothing (2026-11-10 walk).
import { keepNewestFiles, readJson, writeJson } from './files';

const DIR = 'chat-cache';
/** Conversations kept: the ones opened most lately. */
const MOST_CHATS = 40;
/** As many as the first page the computer sends. */
export const CACHED = 50;

export type Cached<M> = { session: string; messages: M[]; saved: number };

const file = (machine: string, terminal: string) => `${`${machine}-${terminal}`.replace(/[^A-Za-z0-9_-]/g, '_')}.json`;

export function cachedChat<M>(machine: string, terminal: string): Cached<M> | null {
  const c = readJson<Cached<M> | null>(DIR, file(machine, terminal), null);
  return c && Array.isArray(c.messages) && c.messages.length ? c : null;
}

export function cacheChat<M>(machine: string, terminal: string, session: string, messages: M[]) {
  if (!messages.length) return;
  writeJson(DIR, file(machine, terminal), { session, messages: messages.slice(-CACHED), saved: Date.now() / 1000 });
  keepNewestFiles(DIR, MOST_CHATS);
}
