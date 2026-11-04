// What you wrote while the computer was out of reach: kept on this phone, shown in the
// inbox's 「未发送」 and under the conversation, and sent as soon as it is back.
import { useSyncExternalStore } from 'react';
import { persisted } from './kv';

export type Unsent = { id: string; machine: string; terminal: string; text: string; at: number; error?: string };

const box = persisted<{ items: Unsent[] }>('mooring.outbox', { items: [] });
export const outboxReady = box.ready;

export function enqueue(u: Omit<Unsent, 'id' | 'at'>) {
  const item = { ...u, id: `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`, at: Date.now() / 1000 };
  box.set((v) => ({ items: [...v.items, item] }));
  return item;
}

export function drop(id: string) {
  box.set((v) => ({ items: v.items.filter((x) => x.id !== id) }));
}

export function mark(id: string, error: string) {
  box.set((v) => ({ items: v.items.map((x) => (x.id === id ? { ...x, error } : x)) }));
}

/** Tries a failed one again: the pump picks it up once its error is gone. */
export function retry(id: string) {
  box.set((v) => ({ items: v.items.map((x) => (x.id === id ? { ...x, error: undefined } : x)) }));
}

export const unsent = () => box.get().items;
export const useOutbox = () => useSyncExternalStore(box.subscribe, box.get).items;
