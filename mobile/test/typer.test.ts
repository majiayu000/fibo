import { expect, test } from 'bun:test';
import { step } from '../src/ui/typer';

function run(from: string, to: string): string[] {
  const seen = [from];
  while (seen[seen.length - 1] !== to) seen.push(step(seen[seen.length - 1], to));
  return seen;
}

test('heard words are typed one at a time, not dropped in at once', () => {
  expect(run('改一下', '改一下测试')).toEqual(['改一下', '改一下测', '改一下测试']);
});

test('a long jump types faster so the box never lags far behind', () => {
  const text = '帮'.repeat(200);
  expect(run('', text).length).toBeLessThan(60);
});

test('words the recognizer took back go at once, then the new ones are typed', () => {
  const seen = run('请我看一下', '帮我看一下');
  expect(seen[1]).toBe('');
  expect(seen[2]).toBe('帮');
  expect(seen[seen.length - 1]).toBe('帮我看一下');
});
