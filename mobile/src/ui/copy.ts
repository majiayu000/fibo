// A copy button's work: the text goes to the clipboard, and a note says so for a moment.
import * as Clipboard from 'expo-clipboard';

/** How long 「已复制」 stays. */
export const NOTE_MS = 3000;

/** Copies `text` as it is and shows `note`; `setNote('')` once it has been read. */
export function copyWithNote(text: string, note: string, setNote: (note: string) => void, gone: () => boolean = () => false) {
  Clipboard.setStringAsync(text).catch(() => undefined);
  setNote(note);
  setTimeout(() => !gone() && setNote(''), NOTE_MS);
}
