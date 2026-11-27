// What dictation puts in a box: the words already there, then what was heard. No React
// Native here, so plain tests can check it.

const CJK = /[　-〿぀-ヿ㐀-鿿豈-﫿＀-￯]/;

/** `before` and `heard` side by side: a space between two Latin words, nothing next to Chinese or after a line break. */
export function joinSpoken(before: string, heard: string): string {
  const said = heard.trim();
  if (!said) return before;
  if (!before) return said;
  const last = before.slice(-1);
  if (/\s/.test(last) || CJK.test(last) || CJK.test(said[0])) return before + said;
  return `${before} ${said}`;
}

/** Errors that only mean it stopped: nothing was said, or this app stopped it. */
export const QUIET_ERRORS = new Set(['aborted', 'no-speech', 'speech-timeout']);
