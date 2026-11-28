import { expect, test } from 'bun:test';
import { joinSpoken } from '../src/ui/spoken';

test('dictation joins what was heard onto the box without doubling or gluing words', () => {
  expect(joinSpoken('', '  帮我跑测试 ')).toBe('帮我跑测试');
  expect(joinSpoken('先看看', '日志')).toBe('先看看日志');
  expect(joinSpoken('run the', 'tests')).toBe('run the tests');
  expect(joinSpoken('fix #12', '然后提交')).toBe('fix #12然后提交');
  expect(joinSpoken('第一行\n', 'second')).toBe('第一行\nsecond');
  expect(joinSpoken('keep', '   ')).toBe('keep');
});
