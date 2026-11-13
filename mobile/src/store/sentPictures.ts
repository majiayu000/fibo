// Copies of the pictures this phone sent, so its own bubbles show them: the computer keeps
// the pictures, and the conversation's log holds only a tag (2026-11-10 walk).
import { useSyncExternalStore } from 'react';
import type { Picture } from '@/ui/pictures';
import { keepNewest, type SentPictures } from '@/ui/sent';
import { keepCopy, readJson, remove, writeJson } from './files';

const DIR = 'sent-pictures';
const INDEX = 'index.json';
/** About a month of sending pictures now and then; each copy is under 1 MB. */
const MOST = 80;

let sent: SentPictures[] = readJson<SentPictures[]>(DIR, INDEX, []);
const listeners = new Set<() => void>();

/** Keeps copies of `pictures`, sent with `words` to the terminal's conversation. */
export function rememberSent(machine: string, terminal: string, words: string, pictures: Picture[]) {
  if (!pictures.length) return;
  const stamp = Date.now().toString(36);
  const files = pictures.map((p, i) => keepCopy(DIR, p.uri, `${stamp}-${i}.jpg`)).filter((f): f is string => !!f);
  if (!files.length) return;
  const { kept, dropped } = keepNewest([...sent, { machine, terminal, words, at: Date.now() / 1000, files }], MOST);
  for (const d of dropped) d.files.forEach(remove);
  sent = kept;
  writeJson(DIR, INDEX, sent);
  listeners.forEach((l) => l());
}

export const useSentPictures = () =>
  useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => sent,
  );
