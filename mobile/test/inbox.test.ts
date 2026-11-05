// Where the inbox puts cards and when a tap counts: a tap must land on what the person saw.
// And it reads 要你处理 off the host, so what the host brings back shows here too.
import { describe, expect, test } from 'bun:test';
import type { Card, Machine } from '../src/remote/hub';
import { HOLD_NONE, SCROLL_QUIET_MS, SETTLE_MS, TOUCH_LIMIT_MS, arrange, holdLeft, settleLeft, settleNext, sort } from '../src/ui/inbox';

const card = (id: string, status: string, updated = 0, more: Partial<Card> = {}) =>
  ({ id, agent: 'claude', status, status_label: status, phase: status === 'waiting' ? 'waiting' : 'working', updated, space: 's', cwd: '/x', ...more }) as Card;
const approval = (id: string, updated = 0) => card(id, 'running', updated, { approval: 'Do you want to proceed?' });
const machine = (cards: Card[], id = 'm', status = 'online') => ({ id, status, cards, spaces: {} }) as unknown as Machine;
const ids = (items: { c: Card }[]) => items.map((i) => i.c.id);

describe('what asks you stays where it is', () => {
  test('approvals and questions go first, in the order they appeared, not by kind', () => {
    const first = arrange([machine([card('run', 'running', 9), approval('a', 5), card('q', 'waiting', 7)])], []);
    expect(ids(first.pinned)).toEqual(['a', 'q']);
    expect(ids(first.rest)).toEqual(['run']);
    // A question that turns into an approval, or the other way round, does not jump the queue.
    const later = arrange([machine([card('a', 'waiting', 50), approval('q', 60)])], first.pins);
    expect(ids(later.pinned)).toEqual(['a', 'q']);
  });
  test('activity never reorders them', () => {
    const pins = arrange([machine([approval('a', 1), approval('b', 2)])], []).pins;
    expect(pins).toEqual(['m/a', 'm/b']);
    // b's log moved on: by most recent activity it would now lead.
    expect(ids(arrange([machine([approval('b', 99), approval('a', 1)])], pins).pinned)).toEqual(['a', 'b']);
  });
  test('a new one goes after the ones already there', () => {
    const pins = arrange([machine([approval('a', 5)])], []).pins;
    const next = arrange([machine([approval('new', 100), approval('a', 5)])], pins);
    expect(ids(next.pinned)).toEqual(['a', 'new']);
    // Older by its log, it still came later.
    const older = arrange([machine([approval('old', 1), approval('a', 5)])], pins);
    expect(ids(older.pinned)).toEqual(['a', 'old']);
  });
  test('several new at once come oldest first, and the same way every time', () => {
    const cards = [approval('c', 3), approval('a', 1), approval('b', 2), approval('t', 2)];
    expect(ids(arrange([machine(cards)], []).pinned)).toEqual(['a', 'b', 't', 'c']);
    expect(ids(arrange([machine([...cards].reverse())], []).pinned)).toEqual(['a', 'b', 't', 'c']);
  });
  test('one answered leaves the others in order; if it asks again it goes last', () => {
    const pins = arrange([machine([approval('a'), approval('b'), approval('c')])], []).pins;
    const without = arrange([machine([card('a', 'running'), approval('b'), approval('c')])], pins);
    expect(ids(without.pinned)).toEqual(['b', 'c']);
    expect(ids(arrange([machine([approval('a'), approval('b'), approval('c')])], without.pins).pinned)).toEqual(['b', 'c', 'a']);
  });
  test('the rest of the list moving does not touch them', () => {
    const before = arrange([machine([approval('a'), card('x', 'running', 1), card('y', 'running', 2), card('e', 'error', 0)])], []);
    expect(ids(before.rest)).toEqual(['e', 'y', 'x']);
    const after = arrange([machine([approval('a'), card('x', 'running', 9), card('y', 'review', 2), card('e', 'running', 0)])], before.pins);
    expect(ids(after.pinned)).toEqual(['a']);
    expect(ids(after.rest)).toEqual(['x', 'e', 'y']);
  });
  test('two computers: same id on each is two cards', () => {
    const a = arrange([machine([approval('t')], 'm1'), machine([approval('t')], 'm2')], []);
    expect(a.pins).toEqual(['m1/t', 'm2/t']);
    expect(arrange([machine([approval('t')], 'm2'), machine([approval('t')], 'm1')], a.pins).pins).toEqual(['m1/t', 'm2/t']);
  });
  test('a computer that left cannot be answered: its cards drop out, and come back after the others', () => {
    const both = [machine([approval('t')], 'm1'), machine([approval('u')], 'm2')];
    const pins = arrange(both, []).pins;
    const gone = arrange([machine([approval('t')], 'm1', 'connecting'), machine([approval('u')], 'm2')], pins);
    expect(gone.pins).toEqual(['m2/u']);
    expect(ids(gone.rest)).toEqual(['t']);
    expect(gone.offline.map((m) => m.id)).toEqual(['m1']);
    expect(arrange(both, gone.pins).pins).toEqual(['m2/u', 'm1/t']);
  });
  test('settled conversations stay out, until they ask again', () => {
    const settled = [card('d', 'review', 1, { outcome: 'done' }), card('l', 'review', 1, { outcome: 'deferred' })];
    const a = arrange([machine(settled)], []);
    expect([ids(a.pinned), ids(a.done), ids(a.later)]).toEqual([[], ['d'], ['l']]);
    const asks = arrange([machine([{ ...settled[0], approval: 'q' }])], []);
    expect(ids(asks.pinned)).toEqual(['d']);
    expect(ids(asks.done)).toEqual([]);
  });
});

describe('a tap right after a change does nothing', () => {
  test('a new card waits, then counts', () => {
    const s = settleNext(null, 'k', 1000);
    expect(settleLeft(s, 1000)).toBe(SETTLE_MS);
    expect(settleLeft(s, 1000 + SETTLE_MS - 1)).toBe(1);
    expect(settleLeft(s, 1000 + SETTLE_MS)).toBe(0);
  });
  test('the same content does not start over, however often it redraws', () => {
    const s = settleNext(null, 'k', 1000);
    expect(settleNext(s, 'k', 5000)).toBe(s);
  });
  test('new content or a new place starts the wait again', () => {
    const s = settleNext(null, 'push|y:0', 1000);
    const moved = settleNext(s, 'push|y:120', 5000);
    expect(settleLeft(moved, 5000)).toBe(SETTLE_MS);
    expect(settleLeft(settleNext(s, 'rm|y:0', 5000), 5001)).toBe(SETTLE_MS - 1);
  });
});

describe('the list holds still under a finger', () => {
  test('nothing holds it by itself', () => {
    expect(holdLeft(HOLD_NONE, 5000)).toBe(0);
  });
  test('a finger holds it until it lifts', () => {
    const down = { touching: true, touchedAt: 1000, scrolledAt: 0 };
    expect(holdLeft(down, 1000 + 3000)).toBeGreaterThan(0);
    expect(holdLeft({ ...down, touching: false }, 1000 + 3000)).toBe(0);
  });
  test('a lost touch-end does not freeze the inbox for good', () => {
    const down = { touching: true, touchedAt: 1000, scrolledAt: 0 };
    expect(holdLeft(down, 1000 + TOUCH_LIMIT_MS - 1)).toBe(1);
    expect(holdLeft(down, 1000 + TOUCH_LIMIT_MS)).toBe(0);
  });
  test('a scroll holds it until it has been still a moment', () => {
    const scrolled = { touching: false, touchedAt: 0, scrolledAt: 1000 };
    expect(holdLeft(scrolled, 1000)).toBe(SCROLL_QUIET_MS);
    expect(holdLeft(scrolled, 1000 + SCROLL_QUIET_MS)).toBe(0);
  });
});

describe('inbox lists', () => {
  const plain = (o: Partial<Card>): Card => ({ id: 'c', space: 's', tab: 't', cwd: '/', phase: 'idle', agent: 'claude', session: 'x', ...o });
  const where = (c: Card) => {
    const { inbox, later, done } = sort([machine([c])]);
    return inbox.length ? 'inbox' : later.length ? 'later' : done.length ? 'done' : 'none';
  };

  test('a 稍后提醒 whose hour is up comes back when the host says it needs you', () => {
    expect(where(plain({ outcome: 'deferred', status: 'review', needs_you: true }))).toBe('inbox');
  });

  test('a 稍后提醒 still waiting its hour stays put', () => {
    expect(where(plain({ outcome: 'deferred', status: 'review', needs_you: false }))).toBe('later');
  });

  test('settled conversations stay settled unless something asks', () => {
    expect(where(plain({ outcome: 'done', status: 'closed', needs_you: false }))).toBe('done');
    expect(where(plain({ outcome: 'done', phase: 'waiting', needs_you: false }))).toBe('inbox');
    expect(where(plain({ outcome: 'deferred', approval: 'Proceed?' }))).toBe('inbox');
  });

  test('shells are not conversations', () => {
    expect(where(plain({ phase: 'shell', agent: null }))).toBe('none');
  });

  test('a 稍后提醒 the host brought back joins the inbox but is not pinned: it asks nothing', () => {
    const back = plain({ id: 'back', outcome: 'deferred', status: 'review', needs_you: true });
    const a = arrange([machine([back, approval('a')])], []);
    expect(ids(a.pinned)).toEqual(['a']);
    expect(ids(a.rest)).toEqual(['back']);
    expect(ids(a.later)).toEqual([]);
  });
});
