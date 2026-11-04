// Which files of a diff you ticked 看过, kept on this phone only. A tick holds for the
// patch it was given to: change the file and it is unticked again.
import { useSyncExternalStore } from 'react';
import { persisted } from './kv';

const ticks = persisted<{ seen: Record<string, string> }>('mooring.reviewed', { seen: {} });

/** A short fingerprint of a patch, enough to notice it changed. */
export function fingerprint(text: string): string {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619) >>> 0;
  return `${text.length.toString(36)}.${h.toString(36)}`;
}

const key = (machine: string, cwd: string, path: string) => `${machine}|${cwd}|${path}`;

export function useReviewed(machine: string, cwd: string) {
  const { seen } = useSyncExternalStore(ticks.subscribe, ticks.get);
  return {
    has: (path: string, patch: string) => seen[key(machine, cwd, path)] === fingerprint(patch),
    toggle(path: string, patch: string) {
      const k = key(machine, cwd, path);
      const on = seen[k] === fingerprint(patch);
      ticks.set((v) => {
        const next = { ...v.seen };
        if (on) delete next[k];
        else next[k] = fingerprint(patch);
        return { seen: next };
      });
    },
  };
}
