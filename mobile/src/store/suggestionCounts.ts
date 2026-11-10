// How Claude's next-prompt suggestion does on this phone: of the turns that ended here, how
// many offered one and how many were tapped. It decides whether the feature stays: dropped
// if under one in ten gets used, or most turns never get one because the computer's terminal
// was not focused when the turn ended. Nothing here reaches the computer.
/** Enough to not count a turn twice when the page is opened again. */
const REMEMBERED = 100;

export type SuggestionStats = { since: number; turns: number; offered: number; used: number; seenTurns: string[]; seenOffers: string[] };

export const NONE: SuggestionStats = { since: 0, turns: 0, offered: 0, used: 0, seenTurns: [], seenOffers: [] };

export const countTurn = (s: SuggestionStats, key: string, now: number): SuggestionStats =>
  s.seenTurns.includes(key)
    ? s
    : { ...s, since: s.since || now, turns: s.turns + 1, seenTurns: [...s.seenTurns, key].slice(-REMEMBERED) };

export const countOffer = (s: SuggestionStats, key: string): SuggestionStats =>
  s.seenOffers.includes(key) ? s : { ...s, offered: s.offered + 1, seenOffers: [...s.seenOffers, key].slice(-REMEMBERED) };

export const countUse = (s: SuggestionStats): SuggestionStats => ({ ...s, used: s.used + 1 });

import { tr } from '../i18n';

export function statsWords(s: SuggestionStats): string {
  if (!s.turns) return tr('还没有记录。Claude 一轮结束、手机正开着这段对话时才计数。');
  const day = new Date(s.since * 1000);
  return tr('{month} 月 {day} 日起：{turns} 轮里 {offered} 轮有建议，点用 {used} 次。', { month: day.getMonth() + 1, day: day.getDate(), turns: s.turns, offered: s.offered, used: s.used });
}
