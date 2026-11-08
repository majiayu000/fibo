import { expect, test } from 'bun:test';
import { sameWords } from '../src/ui/sent';

test('a message with pictures is found wherever the CLI put their tags', () => {
  // Claude Code on the real phone, 11-08: the words first, the tag after.
  expect(sameWords('what colors?[Image #1]', 'what colors?', 1)).toBe(true);
  expect(sameWords('[Image #1] [Image #2] 看这两张', '看这两张', 2)).toBe(true);
  expect(sameWords('[Image 1]  图是什么颜色', '图是什么颜色', 1)).toBe(true);
  expect(sameWords('[Image #1]', '', 1)).toBe(true);
  expect(sameWords('what colors?', 'what colors?', 1)).toBe(false);
  expect(sameWords('[Image #1] something else', 'what colors?', 1)).toBe(false);
});

test('a message without pictures needs the same words', () => {
  expect(sameWords(' 继续 ', '继续')).toBe(true);
  expect(sameWords('继续吧', '继续')).toBe(false);
});
