// A label stops at the floating header's foot, not the screen's top, and the next label pushes it off.
import { expect, test } from 'bun:test';
import { stickRange } from '../src/ui/stickRange';

const at = (r: { inputRange: number[]; outputRange: number[] }, x: number) => {
  const { inputRange: i, outputRange: o } = r;
  if (x <= i[0]) return o[0];
  for (let k = 1; k < i.length; k++) if (x <= i[k]) return o[k - 1] + ((o[k] - o[k - 1]) * (x - i[k - 1])) / (i[k] - i[k - 1] || 1);
  const n = i.length - 1;
  return o[n] + ((o[n] - o[n - 1]) / (i[n] - i[n - 1])) * (x - i[n]);
};

test('sticks at the offset below the top, then rides with the scroll', () => {
  // Label laid out at y=500, 36 tall, header 150: it reaches the header's foot at scroll 350.
  const r = stickRange(500, 36, 900, 150);
  expect(at(r, 0)).toBe(0);
  expect(at(r, 350)).toBe(0);
  expect(500 - 400 + at(r, 400)).toBe(150);
  expect(500 - 600 + at(r, 600)).toBe(150);
});

test('the next label pushes it off', () => {
  const r = stickRange(500, 36, 900, 150);
  // Next at 900 meets this one's foot (150 + 36) at scroll 714; after that both scroll away.
  expect(500 - 714 + at(r, 714)).toBe(150);
  expect(500 - 800 + at(r, 800)).toBe(150 - 86);
});

test('the last label stays stuck, and input never goes down', () => {
  const r = stickRange(500, 36, null, 150);
  expect(500 - 2000 + at(r, 2000)).toBe(150);
  for (const rr of [r, stickRange(100, 36, 120, 150), stickRange(0, 36, 0, 0)])
    rr.inputRange.forEach((v, k) => k && expect(v).toBeGreaterThanOrEqual(rr.inputRange[k - 1]));
});
