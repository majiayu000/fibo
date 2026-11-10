import { expect, test } from 'bun:test';
import { sameWords, sendTarget, shownWords } from '../src/ui/sent';

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

test('a logged message shows its pictures the way it read while sending', () => {
  expect(shownWords('phone-walk: what is on it?[Image #1]')).toBe('[1 张图片] phone-walk: what is on it?');
  expect(shownWords('[Image #1] [Image #2] 看这两张')).toBe('[2 张图片] 看这两张');
  expect(shownWords('[Image #1]')).toBe('[1 张图片]');
  expect(shownWords('没有图')).toBe('没有图');
});

test('written while the computer is away, a message still knows its conversation', () => {
  expect(sendTarget('s1', true, 's2')).toBe('s1');
  expect(sendTarget(null, false, 's2')).toBe('s2');
  // Online the page reads the conversation itself; until then there is none to send to.
  expect(sendTarget(null, true, 's2')).toBe(null);
  expect(sendTarget(null, false, undefined)).toBe(null);
});
