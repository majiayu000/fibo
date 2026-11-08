// Whether a message the phone sent is now in the conversation's log, so its 「发送中」 copy can go.

/** How each CLI writes a pasted picture into the message: Claude Code and Codex `[Image #1]`, OpenCode `[Image 1]`. */
const PICTURE = /\[Image #?\d+\]/;

/**
 * The same words, wherever the pictures' tags landed: Claude Code logs the words first and the
 * tag after them (`what colors?[Image #1]`), however they were pasted.
 */
export function sameWords(logged: string, sent: string, pictures = 0) {
  const words = (s: string) => s.replace(new RegExp(PICTURE, 'g'), ' ').replace(/\s+/g, ' ').trim();
  return pictures ? words(logged) === words(sent) && PICTURE.test(logged) : logged.trim() === sent.trim();
}
