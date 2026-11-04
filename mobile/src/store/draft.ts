// The new task being written: which computer, directory and Agent, and what to ask. Kept
// in memory while you go off to pick a directory and come back.
import { useSyncExternalStore } from 'react';

export type Draft = {
  machine?: string;
  cwd?: string;
  /** What the picker knew of the directory: 「git · main · 2 个改动」. */
  about?: string;
  agent: 'claude' | 'codex';
  prompt: string;
  /** The conversation to pick back up instead of starting one. */
  resume?: { id: string; title: string; updated: number };
};

let draft: Draft = { agent: 'claude', prompt: '' };
const listeners = new Set<() => void>();

export function setDraft(patch: Partial<Draft>) {
  draft = { ...draft, ...patch };
  listeners.forEach((l) => l());
}

export const resetDraft = () => setDraft({ cwd: undefined, about: undefined, prompt: '', resume: undefined });

export const useDraft = () =>
  useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => draft,
  );
