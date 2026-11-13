// Whether a message the phone sent is now in the conversation's log, so its 「发送中」 copy can go.
import { tr } from '../i18n';

/**
 * How a picture shows in the logged message: Claude Code and Codex `[Image #1]`, OpenCode
 * `[Image 1]`; a new task's pictures reach Claude Code as `@<runtime>/phone-images/<file>`.
 */
const PICTURE = /\[Image #?\d+\]|@\S*[\\/]phone-images[\\/]\S+/;

/**
 * The same words, wherever the pictures' tags landed: Claude Code logs the words first and the
 * tag after them (`what colors?[Image #1]`), however they were pasted.
 */
export function sameWords(logged: string, sent: string, pictures = 0) {
  const words = (s: string) => s.replace(new RegExp(PICTURE, 'g'), ' ').replace(/\s+/g, ' ').trim();
  return pictures ? words(logged) === words(sent) && PICTURE.test(logged) : logged.trim() === sent.trim();
}

/**
 * Your message as the bubble shows it: the pictures' tags, which only the CLI reads, become
 * 「[1 张图片]」 up front, as the message read while it was being sent (2026-11-10 walk).
 */
export function shownWords(logged: string) {
  const tags = logged.match(new RegExp(PICTURE, 'g'))?.length ?? 0;
  if (!tags) return logged;
  const words = logged.replace(new RegExp(PICTURE, 'g'), ' ').replace(/\s+/g, ' ').trim();
  return `${tr('[{n} 张图片]', { n: tags })} ${words}`.trim();
}

/**
 * The conversation a message goes to. Opened while the computer is away, the page has not
 * read it yet; the terminal's card, kept from before, still names it, so what you write waits
 * for the computer instead of being refused (2026-11-10 walk).
 */
export function sendTarget(read: string | null, online: boolean, card: string | null | undefined) {
  return read ?? (online ? null : card ?? null);
}

/** Pictures this phone sent with a message: copies kept here, as the log holds only a tag. */
export type SentPictures = { machine: string; terminal: string; words: string; at: number; files: string[] };

/** How far apart the send and the logged message may be and still be the same one. */
const SAME_SEND_S = 600;

/**
 * The copies of the pictures a logged message was sent with, from this phone: same
 * conversation's terminal, the same words, a picture tag in the log, close in time.
 * Pictures sent from elsewhere have none here, and keep their 「[N 张图片]」.
 */
export function picturesFor(sent: SentPictures[], machine: string, terminal: string, logged: string, at: number): string[] | null {
  for (let i = sent.length - 1; i >= 0; i--) {
    const s = sent[i];
    if (s.machine !== machine || s.terminal !== terminal || Math.abs(at - s.at) > SAME_SEND_S) continue;
    if (sameWords(logged, s.words, s.files.length)) return s.files;
  }
  return null;
}

/** The words a bubble shows next to its own thumbnails: the tags, and the count, go. */
export function wordsBeside(logged: string) {
  return logged.replace(new RegExp(PICTURE, 'g'), ' ').replace(/\s+/g, ' ').trim();
}

/** The newest `most` sends; the ones that fall off, whose copies can go. */
export function keepNewest(sent: SentPictures[], most: number): { kept: SentPictures[]; dropped: SentPictures[] } {
  const over = Math.max(0, sent.length - most);
  return { kept: sent.slice(over), dropped: sent.slice(0, over) };
}
