// Column widths for a table on a phone, from its text alone, so drawing never has to measure.
// Pure, so tests run it without React Native.
const CELL_PAD = 20;
const MIN_COLUMN = 56;
const MAX_COLUMN = 220;

const WIDE = /[ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹯＀-｠￠-￦]/;

/** A rough width in points: CJK is a square, Latin about half of one. */
export function textWidth(s: string, size: number): number {
  let w = 0;
  for (const c of s) w += WIDE.test(c) ? size : size * 0.6;
  return w;
}

/** The widest line of monospace text: a CJK fallback glyph is a square, Latin 0.6 of one. */
export function monoWidth(text: string, size: number): number {
  let widest = 0;
  for (const line of text.split('\n')) {
    let w = 0;
    for (const c of line) w += WIDE.test(c) ? size : size * 0.6;
    if (w > widest) widest = w;
  }
  return widest;
}

/** The widest run that cannot wrap: a Latin word or path. CJK breaks anywhere, so one square. */
function unbreakable(s: string, size: number): number {
  let widest = size;
  for (const run of s.split(/[\s⺀-꓏가-힣＀-｠]+/)) widest = Math.max(widest, textWidth(run, size));
  return widest;
}

/** Short enough that a long cell stays a few lines tall, not a thin tower of characters. */
const MOST_LINES = 2.5;

/**
 * Each column's width. A table that fits fills the screen, sharing what is left by how much
 * text each column has. One that does not fit keeps every column readable and scrolls
 * sideways, so a long cell wraps within MAX_COLUMN rather than taking the whole table wide.
 */
export function columnWidths(rows: string[][], size: number, available: number): number[] {
  const count = rows.reduce((n, r) => Math.max(n, r.length), 0);
  const want = new Array<number>(count).fill(0);
  const word = new Array<number>(count).fill(0);
  for (const row of rows) {
    row.forEach((cell, i) => {
      want[i] = Math.max(want[i], textWidth(cell, size) + CELL_PAD);
      word[i] = Math.max(word[i], unbreakable(cell, size) + CELL_PAD);
    });
  }
  // A heading never wraps unless it has to.
  const head = want.map((_, i) => textWidth(rows[0]?.[i] ?? '', size) + CELL_PAD);
  const least = want.map((w, i) => Math.ceil(Math.min(MAX_COLUMN, Math.max(MIN_COLUMN, word[i], head[i], w / MOST_LINES))));
  const capped = want.map((w, i) => Math.max(least[i], Math.min(w, MAX_COLUMN)));
  const total = capped.reduce((a, b) => a + b, 0);
  if (total <= available) {
    const spare = available - total;
    return capped.map((w) => Math.floor(w + (spare * w) / total));
  }
  const floor = least.reduce((a, b) => a + b, 0);
  if (floor >= available) return least;
  // Fits once the wrappable columns give some width back.
  const give = (available - floor) / (total - floor);
  return capped.map((w, i) => Math.floor(least[i] + (w - least[i]) * give));
}
