// The copy buttons on a terminal's last lines and on a tool's output: the whole text as it
// is, and a note that says so and then goes away.
import { expect, mock, test } from 'bun:test';

let copied: string[] = [];
mock.module('expo-clipboard', () => ({
  setStringAsync: async (text: string) => {
    copied.push(text);
    return true;
  },
}));

const { copyWithNote, NOTE_MS } = await import('../src/ui/copy');
const { textRows } = await import('../src/ui/toolview');

const later = globalThis.setTimeout;
const wait = (ms: number) => new Promise((r) => later(r, ms));

test('the text goes out as the terminal has it, not as the rows draw it', () => {
  copied = [];
  const long = `$ ${'x'.repeat(2500)}\n  second line\n\n  fourth`;
  // Rows break a long line for drawing; the copy must not.
  expect(textRows(long).length).toBeGreaterThan(4);
  const notes: string[] = [];
  copyWithNote(long, '已复制', (n) => notes.push(n));
  expect(copied).toEqual([long]);
  expect(notes).toEqual(['已复制']);
});

test('the note goes away, unless the sheet has closed', async () => {
  const timer = globalThis.setTimeout;
  (globalThis as any).setTimeout = (f: () => void, ms: number) => {
    expect(ms).toBe(NOTE_MS);
    return timer(f, 1);
  };
  try {
    const open: string[] = [];
    copyWithNote('a', '已复制', (n) => open.push(n));
    const closed: string[] = [];
    copyWithNote('a', '已复制', (n) => closed.push(n), () => true);
    await wait(10);
    expect(open).toEqual(['已复制', '']);
    expect(closed).toEqual(['已复制']);
  } finally {
    globalThis.setTimeout = timer;
  }
});
