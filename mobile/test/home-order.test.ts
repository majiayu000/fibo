// The home screen's groups in the order they render: what wants you first, 在跑 (which wants
// nothing) after 等你验收. Flipped twice (4459fca, 7c94c6c) before 2026-11-08; see
// docs/decisions/2026-11-08-review-above-running.md before moving it again.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';

const src = readFileSync(new URL('../src/app/index.tsx', import.meta.url), 'utf8');
const at = (marker: string) => {
  const i = src.indexOf(marker);
  expect(i).toBeGreaterThan(-1);
  return i;
};

test('asks, then errors, then 等你验收, then 在跑, then the rest', () => {
  const order = ['pinned.forEach(', 'ahead.forEach(', 'label={foldLabel(review)}', 'label={foldLabel(running)}', 'behind.forEach('].map(at);
  expect(order).toEqual([...order].sort((a, b) => a - b));
});
