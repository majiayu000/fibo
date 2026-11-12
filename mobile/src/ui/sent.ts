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
