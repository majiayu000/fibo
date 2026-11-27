// Keys the wait panel sends to the terminal. No React Native here, so plain tests can check it.

/** Keys that only move or close a menu; any other key can hand the terminal a choice. */
const MOVES = new Set(['up', 'down', 'esc']);

/** These keys can submit a choice to the terminal, so they ask like 允许 does. */
export function submitsChoice(keys: string[]): boolean {
  return keys.some((k) => !MOVES.has(k));
}
