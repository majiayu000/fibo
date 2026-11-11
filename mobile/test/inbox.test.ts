// Where the inbox puts cards and when a tap counts: a tap must land on what the person saw.
// And it reads 要你处理 off the host, so what the host brings back shows here too.
import { describe, expect, test } from 'bun:test';
import type { Card, Machine } from '../src/remote/hub';
import { HOLD_NONE, RUNNING_PREVIEW, connectionLine, namesComputer, pulse, offlineRows, runningShown, searchCards, undoTo, SCROLL_QUIET_MS, SETTLE_MS, TOUCH_LIMIT_MS, arrange, foldLabel, holdLeft, settleLeft, settleNext, shelve, sort } from '../src/ui/inbox';
import { spaceName } from '../src/ui/status';

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
    const abandoned = plain({ outcome: 'abandoned', status: 'closed', needs_you: false });
    expect(where(abandoned)).toBe('none');
    expect(shelve(arrange([machine([abandoned])], []), null).empty).toBe(true);
    expect(where({ ...abandoned, phase: 'waiting' })).toBe('inbox');
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

describe('finished work folds, and does not read as nothing to do', () => {
  const review = (id: string, more: Partial<Card> = {}) => card(id, 'review', 0, { status_label: '等你验收', needs_you: true, ...more });
  const lay = (cards: Card[], selected: string | null = null, m = machine(cards)) => shelve(arrange([m], []), selected);

  test('what is left of the list keeps its order: errors, the fold, what runs, what rests', () => {
    const s = lay([card('paused', 'paused', 9), review('r'), card('run', 'running', 5), card('e', 'error', 1), approval('a')]);
    expect(ids(s.pinned)).toEqual(['a']);
    expect(ids(s.ahead)).toEqual(['e']);
    expect(ids(s.review)).toEqual(['r']);
    expect(ids(s.running)).toEqual(['run']);
    expect(ids(s.behind)).toEqual(['paused']);
  });
  test('only 等你验收 folds: an error, a question and 等后台 stay out', () => {
    const s = lay([review('r'), card('e', 'error'), card('q', 'waiting'), card('bg', 'background')]);
    expect(ids(s.review)).toEqual(['r']);
    expect(ids(s.ahead)).toEqual(['e']);
    expect(ids(s.running)).toEqual(['bg']);
    expect(ids(s.pinned)).toEqual(['q']);
  });
  test('an approval on a finished turn asks: it does not fold', () => {
    const s = lay([review('r', { approval: 'Proceed?' })]);
    expect(ids(s.pinned)).toEqual(['r']);
    expect(s.review).toEqual([]);
  });
  test('a 稍后提醒 whose hour is up does not fold: it comes back ahead of what runs', () => {
    const s = lay([review('due', { outcome: 'deferred' }), card('run', 'running', 9), card('e', 'error')]);
    expect(ids(s.ahead)).toEqual(['e', 'due']);
    expect(ids(s.running)).toEqual(['run']);
    expect(s.review).toEqual([]);
    expect(s.acts).toBe(2);
  });
  test('the title counts what folded: the host says it is yours', () => {
    const s = lay([review('r1'), review('r2'), approval('a'), card('run', 'running')]);
    expect(s.waiting).toBe(3);
    expect(s.acts).toBe(1);
  });
  test('all folded is not 没有要处理的事', () => {
    const s = lay([review('r')]);
    expect(s.empty).toBe(false);
    expect(s.waiting).toBe(1);
    expect(s.acts).toBe(0);
    expect(lay([]).empty).toBe(true);
    expect(lay([card('d', 'closed', 0, { outcome: 'done' })]).empty).toBe(true);
  });
  test('a computer that left answers nothing: it counts for nothing, and still shows', () => {
    const away = machine([approval('a'), review('r'), card('e', 'error')], 'm', 'connecting');
    const s = lay([], null, away);
    expect([s.waiting, s.acts]).toEqual([0, 0]);
    expect(ids(s.ahead)).toEqual(['a', 'e']);
    expect(ids(s.review)).toEqual(['r']);
  });
  test('the fold says the computer\'s word, in the phone\'s language when it can, and its count', async () => {
    expect(foldLabel(lay([review('r1'), review('r2')]).review)).toBe('等你验收 · 2');
    // The word is the computer's (core's Status::label): an English computer's stays English.
    expect(foldLabel(lay([review('r', { status_label: 'Ready for review' })]).review)).toBe('Ready for review · 1');
    // A Chinese one reads in English on an English phone.
    const { setLanguage } = await import('../src/i18n');
    setLanguage('en');
    expect(foldLabel(lay([review('r1')]).review)).toBe('Ready for review · 1');
    setLanguage('auto');
    // Words the phone has no English for stay as they came, or just the count.
    expect(foldLabel([{ c: card('x', 'mystery', 0, { status_label: 'in review' }) } as never])).toBe('in review · 1');
    expect(foldLabel([{ c: card('x', 'mystery', 0, { status_label: null }) } as never])).toBe('1');
  });
});

describe('the space filter hides nothing silently', () => {
  const at = (space: string, c: Card) => ({ ...c, cwd: `/code/${space}` });
  const two = () => [at('a', approval('ask-a')), at('b', approval('ask-b')), at('b', card('err-b', 'error')), at('a', card('run-a', 'running')), at('b', card('rev-b', 'review', 0, { needs_you: true }))];
  const lay = (cards: Card[], selected: string | null) => shelve(arrange([machine(cards)], []), selected);

  test('one space has no filter, whatever was saved', () => {
    const s = lay([at('a', approval('x')), at('a', card('y', 'running'))], 'a');
    expect(s.chips).toEqual([]);
    expect(s.filter).toBeNull();
    expect(ids(s.pinned)).toEqual(['x']);
  });
  test('two spaces: 全部 first, then by name, with their inbox and what is yours', () => {
    expect(lay(two(), null).chips).toEqual([
      { name: null, count: 5, acts: 3 },
      { name: 'a', count: 2, acts: 1 },
      { name: 'b', count: 3, acts: 2 },
    ]);
  });
  test('a chip does not move when its space gets busier', () => {
    const names = (cs: Card[]) => lay(cs, null).chips.map((c) => c.name);
    expect(names([at('z', approval('1')), at('z', approval('2')), at('b', card('3', 'running'))])).toEqual([null, 'b', 'z']);
  });
  test('a space with only settled conversations still has its chip, without a number to act on', () => {
    const s = lay([at('a', card('run', 'running')), at('b', card('d', 'closed', 0, { outcome: 'done' }))], 'b');
    expect(s.chips.map((c) => [c.name, c.count])).toEqual([[null, 1], ['a', 1], ['b', 0]]);
    expect(ids(s.done)).toEqual(['d']);
    expect(s.empty).toBe(true);
  });
  test('filtered: every list narrows, and what asks elsewhere is counted', () => {
    const cards = [...two(), at('b', card('l', 'review', 0, { outcome: 'deferred' })), at('a', card('d', 'closed', 0, { outcome: 'done' })), at('b', card('d2', 'closed', 0, { outcome: 'done' }))];
    const s = lay(cards, 'a');
    expect(ids(s.pinned)).toEqual(['ask-a']);
    expect(ids(s.running)).toEqual(['run-a']);
    expect(s.review).toEqual([]);
    expect(s.later).toEqual([]);
    expect(ids(s.done)).toEqual(['d']);
    expect(s.elsewhere).toBe(2);
  });
  test('finished work elsewhere is not 等你 in the banner, but the title still counts it', () => {
    const s = lay([at('a', card('run', 'running')), at('b', card('r', 'review', 0, { needs_you: true }))], 'a');
    expect(s.elsewhere).toBe(0);
    expect(s.waiting).toBe(1);
  });
  test('no filter, no banner', () => {
    expect(lay(two(), null).elsewhere).toBe(0);
  });
  test('a saved space that has gone reads as all', () => {
    const s = lay(two(), 'gone');
    expect(s.filter).toBeNull();
    expect(s.pinned.length).toBe(2);
    expect(s.elsewhere).toBe(0);
  });
  test('a filter never touches the order of what asks you', () => {
    const cards = two();
    const a = arrange([machine(cards)], []);
    const before = [...a.pins];
    shelve(a, 'a');
    expect(a.pins).toEqual(before);
    // Switching back after the filter shows the pins as they were.
    expect(ids(shelve(arrange([machine(cards)], a.pins), null).pinned)).toEqual(['ask-a', 'ask-b']);
  });
  test('one project on two computers is one chip, and a filter shows both', () => {
    const m1 = machine([at('a', approval('t'))], 'm1');
    const m2 = machine([at('a', approval('u')), at('b', card('v', 'running'))], 'm2');
    const s = shelve(arrange([m1, m2], []), 'a');
    expect(s.chips.map((c) => c.name)).toEqual([null, 'a', 'b']);
    expect(ids(s.pinned)).toEqual(['t', 'u']);
  });
});

describe('the home screen', () => {
  const one = (cards: Card[], status = 'online', name = 'm') => ({ ...machine(cards, name, status), name });

  test('under the mark, what is going on: most urgent first, two kinds at most', () => {
    const words = (status: string, label: string) => ({ status_label: label, ...(status === 'review' && { needs_you: true }) });
    const run = (id: string) => card(id, 'running', 0, words('running', '在跑'));
    const rev = (id: string) => card(id, 'review', 0, words('review', '等你验收'));
    const ask = card('q', 'waiting', 0, words('waiting', '等你回答'));
    expect(pulse(shelve(arrange([one([run('a'), run('b'), rev('c'), ask])], []), null))).toEqual({ text: '1 个等你回答 · 1 个等你验收', urgent: true });
    expect(pulse(shelve(arrange([one([rev('c'), rev('d'), run('a')])], []), null))).toEqual({ text: '2 个等你验收 · 1 个在跑', urgent: false });
    expect(pulse(shelve(arrange([one([])], []), null))).toEqual({ text: '都歇着', urgent: false });
    // A space shown alone: the line says that space, as the list under it does.
    const at = (space: string, c: Card) => ({ ...c, cwd: `/code/${space}` });
    const two = arrange([machine([at('a', rev('r1')), at('b', run('x')), at('b', ask)])], []);
    expect(pulse(shelve(two, 'a')).text).toBe('1 个等你验收');
  });

  test('what runs shows a few at first, all when asked', () => {
    const items = [1, 2, 3, 4, 5, 6];
    expect(runningShown(items, false).length).toBe(RUNNING_PREVIEW);
    expect(runningShown(items, true)).toEqual(items);
    expect(runningShown([1], false)).toEqual([1]);
  });

  test('the line says which computer, how many are off, and when none can hear you', () => {
    expect(connectionLine([])).toEqual({ text: '', alert: false });
    expect(connectionLine([{ name: 'mac', status: 'online' }])).toEqual({ text: 'mac 在线', alert: false });
    expect(connectionLine([{ name: 'mac', status: 'online' }, { name: 'x', status: 'offline' }, { name: 'y', status: 'offline' }]).text).toBe('mac 在线 · 2 台离线');
    expect(connectionLine([{ name: 'a', status: 'online' }, { name: 'b', status: 'online' }]).text).toBe('2 台电脑在线');
    expect(connectionLine([{ name: 'mac', status: 'offline' }])).toEqual({ text: 'mac不在线', alert: true });
    expect(connectionLine([{ name: 'a', status: 'offline' }, { name: 'b', status: 'offline' }]).alert).toBe(true);
    expect(connectionLine([{ name: 'a', status: 'online' }, { name: 'b', status: 'refused' }]).alert).toBe(true);
  });

  test('a computer that is off takes a row only when nothing else can hear you, or it refused this phone', () => {
    const on = { name: 'a', status: 'online' as const };
    const off = { name: 'b', status: 'offline' as const };
    const refused = { name: 'c', status: 'refused' as const };
    expect(offlineRows([on, off])).toEqual([]);
    expect(offlineRows([on, off, refused])).toEqual([refused]);
    expect(offlineRows([off])).toEqual([off]);
    expect(offlineRows([])).toEqual([]);
  });

  test('the computer is named on a row only when more than one is online', () => {
    expect(namesComputer([{ name: 'a', status: 'online' }, { name: 'b', status: 'offline' }])).toBe(false);
    expect(namesComputer([{ name: 'a', status: 'online' }, { name: 'b', status: 'online' }])).toBe(true);
  });

  test('撤销 puts it back where it was', () => {
    expect(undoTo({})).toBe('open');
    expect(undoTo({ outcome: null })).toBe('open');
    expect(undoTo({ outcome: 'deferred' })).toBe('deferred');
    expect(undoTo({ outcome: 'done' })).toBe('done');
    expect(undoTo({ outcome: 'abandoned' })).toBe('abandoned');
  });

  test('search finds title, space and directory, in 稍后提醒 and 已处理 too, narrowed to a space', () => {
    const m = {
      ...one([
        card('a', 'running', 0, { title: '修好手机流量', cwd: '/w/mooring' }),
        card('b', 'running', 0, { title: 'Other', cwd: '/w/her', outcome: 'deferred' }),
        card('c', 'review', 0, { title: '流量统计', cwd: '/w/lite', outcome: 'done' }),
      ]),
    };
    const a = arrange([m], []);
    expect(ids(searchCards(a, '流量', null)).sort()).toEqual(['a', 'c']);
    expect(ids(searchCards(a, 'OTHER', null))).toEqual(['b']);
    expect(ids(searchCards(a, 'w/her', null))).toEqual(['b']);
    expect(ids(searchCards(a, '  手机  流量 ', null))).toEqual(['a']);
    expect(searchCards(a, '没有这个', null)).toEqual([]);
    expect(searchCards(a, '', null).length).toBe(3);
    const space = spaceName(m, m.cards[0]);
    expect(searchCards(a, '', space).every((i) => spaceName(i.m, i.c) === space)).toBe(true);
    expect(searchCards(a, '', '不存在的空间')).toEqual([]);
  });
});
