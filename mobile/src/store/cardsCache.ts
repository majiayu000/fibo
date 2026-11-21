// Each computer's last list of conversations, kept in this phone's documents folder: opened
// cold while the computer is away, the home screen shows those rows marked 「断开前：…」
// instead of nothing (11-21 review). Only ever what that computer said itself.
import type { CardShelf, SavedCards } from '../remote/hub';
import { dropJson, readJson, writeJson } from './files';

const DIR = 'cards-cache';
const file = (machine: string) => `${machine.replace(/[^A-Za-z0-9_-]/g, '_')}.json`;

export const cardShelf: CardShelf = {
  read: (machine) => {
    const saved = readJson<SavedCards | null>(DIR, file(machine), null);
    return saved && Array.isArray(saved.cards) ? saved : null;
  },
  write: (machine, saved) => writeJson(DIR, file(machine), saved),
  drop: (machine) => dropJson(DIR, file(machine)),
};
