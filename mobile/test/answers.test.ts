// An approval's buttons sit in one order on the home row, in the conversation and on its page,
// and a row offers 允许 only when it shows all of the command (11-21 review).
import { expect, test } from 'bun:test';
import { PAGE_ANSWERS, PREVIEW_LINES, rowAnswers } from '../src/ui/answers';

const order = (xs: readonly string[]) => xs.filter((x) => x === 'deny' || x === 'allow');

test('允许 is last and 拒绝 before it, on a row as on the page', () => {
  const row = rowAnswers({ truncated: false }, 2, true);
  expect(row.at(-1)).toBe('allow');
  expect(order(row)).toEqual(order(PAGE_ANSWERS));
});

test('a command cut in the row has no 允许 there, only 看全文 to the page', () => {
  const cut = rowAnswers({ truncated: false }, PREVIEW_LINES + 1, true);
  expect(cut).not.toContain('allow');
  expect(cut).toContain('whole');
  expect(rowAnswers({ truncated: false }, PREVIEW_LINES, true)).toContain('allow');
  // Not yet measured, or the computer could not read all of it: not whole either.
  expect(rowAnswers({ truncated: false }, null, true)).not.toContain('allow');
  expect(rowAnswers({ truncated: true }, 1, true)).not.toContain('allow');
});

test('a phone that cannot answer gets 详情 alone', () => {
  expect(rowAnswers({ truncated: false }, 1, false)).toEqual(['detail']);
});
