// A phone is told once, only about what the computer says asks for you, and nothing about the
// conversation reaches anything but this phone's own notification.
import { describe, expect, test } from 'bun:test';
import { setLanguage } from '../src/i18n';
import { FLICKER_MS, followAlerts, keepsConnected, kindOf, type Memory, NO_MEMORY, step, type StepContext } from '../src/remote/alerts';
import type { Card, Machine } from '../src/remote/hub';

const NOTIFY = { approval: true, waiting: true, error: true, done: false, quiet: false };
const NOON = new Date(2026, 10, 7, 12, 0, 0);
const later = (ms: number) => new Date(NOON.getTime() + ms);
const ctx = (over: Partial<StepContext> = {}): StepContext => ({ notify: NOTIFY, silent: false, hideBody: false, at: NOON, ...over });

const card = (over: Partial<Card> = {}): Card => ({
  id: 't1',
  space: 's',
  tab: 'tab',
  cwd: '/x',
  phase: 'waiting',
  session: 'a',
  turn: '1',
  title: '修登录页',
  status: 'waiting',
  status_label: '等你回答',
  needs_you: true,
  ...over,
});
const running = (over: Partial<Card> = {}) => card({ phase: 'working', status: 'running', status_label: '在跑', needs_you: false, ...over });

const mac = (cards: Card[], over: Partial<Machine> = {}): Machine => ({
  id: 'mac',
  link: {} as Machine['link'],
  name: 'Mac',
  status: 'online',
  ops: [],
  spaces: {},
  updated: 1,
  cards,
  ...over,
});

/** Where a machine stands after its first list: the base later changes are told against. */
const based = (cards: Card[] = []): Memory => step(NO_MEMORY, [mac(cards)], ctx()).memory;

describe('turned on', () => {
  test('only by this switch: a phone saved before it, or by the 11-07 trial build, reads as off', () => {
    // `load` lays a saved value over the defaults, so a field it lacks is the default.
    expect(keepsConnected({})).toBe(false);
    const trial: object = { keepConnected: true };
    expect(keepsConnected(trial)).toBe(false);
    expect(keepsConnected({ backgroundAlerts: false })).toBe(false);
    expect(keepsConnected({ backgroundAlerts: true })).toBe(true);
    // Anything but true, as a hand-edited or damaged value might be, is off.
    const odd: object = { backgroundAlerts: 'yes' };
    expect(keepsConnected(odd)).toBe(false);
  });
});

describe('what counts', () => {
  test('the host decides: a card it says does not ask for you never rings, whatever its state', () => {
    // 先不管 and hidden conversations reach the phone as waiting with needs_you false.
    expect(kindOf(card({ needs_you: false }))).toBeNull();
    expect(step(based(), [mac([card({ needs_you: false })])], ctx()).ring).toEqual([]);
  });

  test('each state goes under its own switch', () => {
    expect(kindOf(card({ approval: 'git push' }))).toBe('approval');
    expect(kindOf(card())).toBe('waiting');
    expect(kindOf(card({ status: 'error', status_label: '出错了' }))).toBe('error');
    expect(kindOf(card({ status: 'review', status_label: '等你验收' }))).toBe('done');
    expect(kindOf(card({ status: 'background', status_label: '等后台' }))).toBe('done');
    expect(kindOf(running({ needs_you: true }))).toBeNull();
  });

  test('a finished turn stays quiet by default: its switch starts off', () => {
    const review = card({ status: 'review', status_label: '等你验收' });
    expect(step(based(), [mac([review])], ctx()).ring).toEqual([]);
    const on = { ...NOTIFY, done: true };
    expect(step(based(), [mac([review])], ctx({ notify: on })).ring).toHaveLength(1);
  });

  test('免打扰 holds everything but an approval at night', () => {
    const night = ctx({ notify: { ...NOTIFY, quiet: true }, at: new Date(2026, 10, 7, 23, 30) });
    expect(step(based(), [mac([card()])], night).ring).toEqual([]);
    expect(step(based(), [mac([card({ approval: 'git push' })])], night).ring).toHaveLength(1);
    const noon = ctx({ notify: { ...NOTIFY, quiet: true } });
    expect(step(based(), [mac([card()])], noon).ring).toHaveLength(1);
  });
});

describe('told once', () => {
  test('what already waits when it is turned on is not news', () => {
    const first = step(NO_MEMORY, [mac([card(), card({ id: 't2' })])], ctx());
    expect(first.ring).toEqual([]);
    expect(step(first.memory, [mac([card(), card({ id: 't2' })])], ctx()).ring).toEqual([]);
  });

  test('a machine online with no list yet is no base: the first real list would ring everything', () => {
    const empty = step(NO_MEMORY, [mac([], { updated: undefined })], ctx());
    expect(empty.memory.seen.mac).toBeUndefined();
    expect(step(empty.memory, [mac([card()])], ctx()).ring).toEqual([]);
  });

  test('a new ask rings once, and again only as a new ask', () => {
    const first = step(based([running()]), [mac([card()])], ctx());
    expect(first.ring).toEqual([{ tag: 'mac/t1', title: '等你回答', body: '修登录页' }]);
    const same = step(first.memory, [mac([card()])], ctx());
    expect(same.ring).toEqual([]);
    expect(same.memory).toBe(first.memory);
    const next = step(same.memory, [mac([card({ approval: 'cargo test', status_label: undefined })])], ctx());
    expect(next.ring).toHaveLength(1);
    expect(next.ring[0].title).toBe('等你审批');
  });

  test('the same words in a later turn are a new ask, however soon', () => {
    const first = step(based([running()]), [mac([card()])], ctx());
    const answered = step(first.memory, [mac([running({ turn: '2' })])], ctx({ at: later(1000) }));
    expect(step(answered.memory, [mac([card({ turn: '2' })])], ctx({ at: later(2000) })).ring).toHaveLength(1);
  });

  test('a question the screen later reads as an approval is one ask: put right, not rung again', () => {
    // On the real phone: 等你回答 from the hook, 等你审批 5.6 s later once the screen was read.
    const told = step(based(), [mac([card()])], ctx());
    expect(told.ring.map((a) => a.title)).toEqual(['等你回答']);
    const read = step(told.memory, [mac([card({ approval: 'Do you want to proceed?' })])], ctx({ at: later(6_000) }));
    expect(read.ring.map((a) => [a.title, a.quiet])).toEqual([['等你审批', true]]);
    expect(read.clear).toEqual([]);
  });

  test('but a real approval still rings: not told of the question, a later turn, or long after', () => {
    const approval = (over: Partial<Card> = {}) => card({ approval: '用 Bash 运行', ...over });
    const told = step(based(), [mac([card()])], ctx());
    // 免打扰 held the question, so nothing has rung yet.
    const night = new Date(2026, 10, 7, 23, 30, 0);
    const held = step(based(), [mac([card()])], ctx({ notify: { ...NOTIFY, quiet: true }, at: night }));
    expect(held.ring).toEqual([]);
    expect(step(held.memory, [mac([approval()])], ctx({ at: new Date(night.getTime() + 6_000) })).ring[0].quiet).toBeUndefined();
    expect(step(told.memory, [mac([approval({ turn: '2' })])], ctx({ at: later(6_000) })).ring[0].quiet).toBeUndefined();
    expect(step(told.memory, [mac([approval()])], ctx({ at: later(FLICKER_MS) })).ring[0].quiet).toBeUndefined();
    // A second approval after the first was put right is a new ask.
    const right = step(told.memory, [mac([approval()])], ctx({ at: later(6_000) }));
    expect(step(right.memory, [mac([approval({ approval: '用 Bash 再运行' })])], ctx({ at: later(8_000) })).ring[0].quiet).toBeUndefined();
    // Answered, the notification comes down, and what follows is news.
    const answered = step(told.memory, [mac([running()])], ctx({ at: later(2_000) }));
    expect(step(answered.memory, [mac([approval()])], ctx({ at: later(4_000) })).ring[0].quiet).toBeUndefined();
  });

  test('a memory saved before it noted what rang still works', () => {
    const { seen, shown } = step(based(), [mac([card()])], ctx()).memory;
    const r = step({ seen, shown }, [mac([card({ approval: '用 Bash 运行' })])], ctx({ at: later(6_000) }));
    expect(r.ring.map((a) => a.quiet)).toEqual([undefined]);
  });

  test('an ask that blinks is one ask; one that truly ended and came back is another', () => {
    const first = step(based([running()]), [mac([card()])], ctx());
    const gone = step(first.memory, [mac([running()])], ctx({ at: later(1000) }));
    expect(step(gone.memory, [mac([card()])], ctx({ at: later(2000) })).ring).toEqual([]);
    const away = step(gone.memory, [mac([running()])], ctx({ at: later(FLICKER_MS + 2000) }));
    expect(step(away.memory, [mac([card()])], ctx({ at: later(FLICKER_MS + 3000) })).ring).toHaveLength(1);
  });

  test('losing the connection forgets nothing: coming back tells only what is new', () => {
    const first = step(based([running()]), [mac([card()])], ctx());
    const off = step(first.memory, [mac([], { status: 'offline' })], ctx());
    expect(off.memory.seen.mac).toEqual(first.memory.seen.mac);
    expect(off.clear).toEqual([]);
    const back = step(off.memory, [mac([card(), card({ id: 't2', title: '新的' })])], ctx());
    expect(back.ring.map((a) => a.body)).toEqual(['新的']);
  });

  test('looking at the app counts as told', () => {
    const looking = step(based([running()]), [mac([card()])], ctx({ silent: true }));
    expect(looking.ring).toEqual([]);
    expect(step(looking.memory, [mac([card()])], ctx()).ring).toEqual([]);
  });
});

describe('taken down', () => {
  test('when the card stops asking, or its computer is removed', () => {
    const first = step(based([running()]), [mac([card()])], ctx());
    expect(step(first.memory, [mac([running()])], ctx()).clear).toEqual(['mac/t1']);
    expect(step(first.memory, [], ctx()).clear).toEqual(['mac/t1']);
  });

  test('not while the computer is out of reach: it may still be asking', () => {
    const first = step(based([running()]), [mac([card()])], ctx());
    expect(step(first.memory, [mac([card()], { status: 'offline' })], ctx()).clear).toEqual([]);
  });
});

describe('what it says', () => {
  test('never the command it asks to run', () => {
    const asked = step(based(), [mac([card({ approval: 'rm -rf secret-dir' })])], ctx()).ring[0];
    expect(JSON.stringify(asked)).not.toContain('secret-dir');
  });

  test('with the app locked, not which conversation', () => {
    expect(step(based(), [mac([card()])], ctx({ hideBody: true })).ring[0].body).toBe('');
  });

  test('in the language chosen in settings, the computer\'s state words included', () => {
    setLanguage('en');
    try {
      const asked = step(based(), [mac([card({ approval: 'git push' })])], ctx()).ring[0];
      expect(asked.title).toBe('Needs approval');
      expect(step(based(), [mac([card()])], ctx()).ring[0].title).toBe('Waiting for you');
    } finally {
      setLanguage('auto');
    }
    expect(step(based(), [mac([card()])], ctx()).ring[0].title).toBe('等你回答');
  });
});

test('following the hub: each change is walked, memory saved, and stopping stops it', () => {
  let machines = [mac([running()])];
  let fire = () => {};
  const shown: string[] = [];
  const hidden: string[] = [];
  const saved: Memory[] = [];
  const stop = followAlerts({
    memory: NO_MEMORY,
    machines: () => machines,
    subscribe: (l) => {
      fire = l;
      return () => {
        fire = () => {};
      };
    },
    context: () => ctx(),
    save: (m) => saved.push(m),
    show: (a) => shown.push(a.body),
    hide: (t) => hidden.push(t),
  });
  expect(saved).toHaveLength(1);
  machines = [mac([card()])];
  fire();
  expect(shown).toEqual(['修登录页']);
  // A heartbeat changes the hub but no card: nothing to do, nothing written to the keystore.
  const writes = saved.length;
  fire();
  expect(saved).toHaveLength(writes);
  machines = [mac([running()])];
  fire();
  expect(hidden).toEqual(['mac/t1']);
  stop();
  machines = [mac([card()])];
  fire();
  expect(shown).toEqual(['修登录页']);
});
