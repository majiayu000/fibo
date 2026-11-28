import { describe, expect, test } from 'bun:test';
import { boundaryBefore, frameEnds, toBase64 } from '../src/ui/adts';

/** One ADTS frame of `len` bytes (header included). */
function frame(len: number): number[] {
  const h = [0xff, 0xf1, 0x60, 0x40 | ((len >> 11) & 0x03), (len >> 3) & 0xff, ((len & 0x07) << 5) | 0x1f, 0xfc];
  return [...h, ...new Array(len - 7).fill(0x21)];
}

describe('adts', () => {
  test('whole frames end where their headers say; a frame still being written is left out', () => {
    const b = new Uint8Array([...frame(20), ...frame(30), ...frame(25).slice(0, 12)]);
    expect(frameEnds(b)).toEqual([20, 50]);
    expect(frameEnds(b, 20)).toEqual([50]);
  });

  test('a slice from one boundary to another is itself a run of whole frames', () => {
    const b = new Uint8Array([...frame(20), ...frame(30), ...frame(25)]);
    const ends = frameEnds(b);
    const start = boundaryBefore(ends, 0, 40);
    expect(start).toBe(20);
    expect(frameEnds(b.subarray(start, ends.at(-1)))).toEqual([30, 55]);
  });

  test('base64 matches the standard encoding across chunk edges', () => {
    const b = new Uint8Array(70000).map((_, i) => i % 251);
    expect(toBase64(b)).toBe(Buffer.from(b).toString('base64'));
  });
});
