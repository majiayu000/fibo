// Which answer buttons an approval shows, left to right, the same everywhere: 拒绝 then 允许,
// 允许 last, where the approval page has always had it (11-21 review: the home row had 允许 on
// the left, the page on the right, so a thumb used to one pressed the other). Pure, for tests.
import type { Ask } from './parse';

/** Lines of the command a row shows. */
export const PREVIEW_LINES = 3;

export type AnswerButton = 'detail' | 'deny' | 'allow' | 'whole';

/** The approval page's pair. */
export const PAGE_ANSWERS = ['deny', 'allow'] as const satisfies readonly AnswerButton[];

/**
 * A row's buttons. 允许 only when all of the command shows in the row: `lines` is how many it
 * takes there (null until measured), and one the computer could not read whole never counts.
 * Otherwise 看全文 opens the page, where all of it shows, to be allowed there.
 */
export function rowAnswers(ask: Pick<Ask, 'truncated'>, lines: number | null, canAnswer: boolean): AnswerButton[] {
  if (!canAnswer) return ['detail'];
  const whole = !ask.truncated && lines !== null && lines <= PREVIEW_LINES;
  return whole ? ['detail', 'deny', 'allow'] : ['deny', 'whole'];
}
