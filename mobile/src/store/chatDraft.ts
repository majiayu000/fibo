// What is written in a conversation's box and not sent yet, kept per terminal. Held by the
// composer alone it went with the chat screen: back to the home screen and in again, the
// words were gone, and a deep link to another conversation carried them along.
// In memory only: it does not outlive the app.
import type { Picture } from '@/ui/pictures';

export type ChatDraft = { text: string; pictures: Picture[] };

const NONE: ChatDraft = { text: '', pictures: [] };
const all = new Map<string, ChatDraft>();
const keyOf = (machine: string, terminal: string) => `${machine}\n${terminal}`;

/** What the terminal's box held when it was last seen. */
export const chatDraft = (machine: string, terminal: string) => all.get(keyOf(machine, terminal)) ?? NONE;

/** The box as it is now; an empty one keeps nothing. */
export function keepChatDraft(machine: string, terminal: string, draft: ChatDraft) {
  const key = keyOf(machine, terminal);
  if (draft.text || draft.pictures.length) all.set(key, draft);
  else all.delete(key);
}
