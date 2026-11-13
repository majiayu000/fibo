import { expect, test } from 'bun:test';
import { keepNewest, picturesFor, sameWords, sendTarget, shownWords, wordsBeside, type SentPictures } from '../src/ui/sent';

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
  // A new task's pictures, as Claude Code logs its first message.
  expect(shownWords('看这两张\n\n@/u/.local/share/fibo/phone-images/a.jpg @/u/.local/share/fibo/phone-images/b.jpg')).toBe('[2 张图片] 看这两张');
  expect(shownWords('@me 你好')).toBe('@me 你好');
});

test('written while the computer is away, a message still knows its conversation', () => {
  expect(sendTarget('s1', true, 's2')).toBe('s1');
  expect(sendTarget(null, false, 's2')).toBe('s2');
  // Online the page reads the conversation itself; until then there is none to send to.
  expect(sendTarget(null, true, 's2')).toBe(null);
  expect(sendTarget(null, false, undefined)).toBe(null);
});

test('a bubble shows the pictures this phone sent with it, and only those', () => {
  const sent: SentPictures[] = [
    { machine: 'm', terminal: 't', words: '看这两张', at: 1000, files: ['a.jpg', 'b.jpg'] },
    { machine: 'm', terminal: 't', words: '再看一张', at: 1100, files: ['c.jpg'] },
  ];
  // Claude Code in a chat logs the words, then the tags.
  expect(picturesFor(sent, 'm', 't', '看这两张[Image #1][Image #2]', 1003)).toEqual(['a.jpg', 'b.jpg']);
  // A new task: Claude Code's `@path` after the words, Codex's tag before them.
  expect(picturesFor(sent, 'm', 't', '再看一张\n\n@/u/fibo/phone-images/x.jpg', 1104)).toEqual(['c.jpg']);
  expect(picturesFor(sent, 'm', 't', '[Image #1]\n再看一张', 1104)).toEqual(['c.jpg']);
  // The same words without a picture, another terminal, or long after: not this send.
  expect(picturesFor(sent, 'm', 't', '看这两张', 1003)).toBe(null);
  expect(picturesFor(sent, 'm', 'other', '看这两张[Image #1][Image #2]', 1003)).toBe(null);
  expect(picturesFor(sent, 'm', 't', '看这两张[Image #1][Image #2]', 5000)).toBe(null);
  expect(wordsBeside('[Image #1] [Image #2] 看这两张')).toBe('看这两张');
});

test('the oldest sends fall off, so their copies can be deleted', () => {
  const s = (n: number): SentPictures => ({ machine: 'm', terminal: 't', words: String(n), at: n, files: [`${n}.jpg`] });
  const { kept, dropped } = keepNewest([s(1), s(2), s(3)], 2);
  expect(kept.map((x) => x.at)).toEqual([2, 3]);
  expect(dropped.map((x) => x.at)).toEqual([1]);
});
