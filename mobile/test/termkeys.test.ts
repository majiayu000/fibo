import { expect, test } from 'bun:test';
import { submitsChoice } from '../src/ui/termkeys';

test('only keys that move or close a menu skip the approval check', () => {
  expect(submitsChoice(['up'])).toBe(false);
  expect(submitsChoice(['down', 'esc'])).toBe(false);
  expect(submitsChoice(['enter'])).toBe(true);
  expect(submitsChoice(['2'])).toBe(true);
});
