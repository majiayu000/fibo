import { describe, expect, test } from 'bun:test';
import { countOffer, countTurn, countUse, statsWords, type SuggestionStats } from '../src/store/suggestionCounts';

const none: SuggestionStats = { since: 0, turns: 0, offered: 0, used: 0, seenTurns: [], seenOffers: [] };

describe('suggestion stats', () => {
  test('a turn and its offer count once, however often the page polls or reopens', () => {
    let s = countTurn(none, 'm/s/1', 1000);
    s = countTurn(s, 'm/s/1', 2000);
    s = countOffer(countOffer(s, 'm/s/1'), 'm/s/1');
    expect([s.turns, s.offered, s.since]).toEqual([1, 1, 1000]);
    s = countUse(countTurn(s, 'm/s/2', 3000));
    expect([s.turns, s.offered, s.used, s.since]).toEqual([2, 1, 1, 1000]);
  });

  test('what it remembers stays bounded', () => {
    let s = none;
    for (let i = 0; i < 150; i++) s = countOffer(countTurn(s, `t${i}`, 1), `t${i}`);
    expect([s.turns, s.offered, s.seenTurns.length, s.seenOffers.length]).toEqual([150, 150, 100, 100]);
  });

  test('says the numbers the decision needs', () => {
    expect(statsWords(none)).toContain('还没有记录');
    const s = { ...none, since: new Date(2026, 10, 6).getTime() / 1000, turns: 20, offered: 5, used: 1 };
    expect(statsWords(s)).toBe('11 月 6 日起：20 轮里 5 轮有建议，点用 1 次。');
  });
});
