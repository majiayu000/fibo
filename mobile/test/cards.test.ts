// A busy computer used to cost the phone its whole list per event: 54–76 MB an hour measured on
// a real phone with 25 terminals. It now sends what changed, no more often than GAP_MS.
import { beforeEach, expect, mock, test } from 'bun:test';
import { GAP_MS, MERGE_MS, patch, refreshWait } from '../src/remote/cards';

mock.module('react-native', () => ({
  AppState: { addEventListener: () => ({ remove() {} }) },
  Platform: { OS: 'web' },
}));
mock.module('../src/store/machines', () => ({
  loadMachines: async () => [],
  saveMachine: async () => undefined,
  forgetMachine: async () => undefined,
  machineId: (l: any) => l.machine,
}));
mock.module('../src/store/quota', () => ({ forgetQuota: async () => undefined }));
const hub = await import('../src/remote/hub');

const a = { id: 'a', v: 1 };
const b = { id: 'b', v: 1 };
const c = { id: 'c', v: 1 };

test('nothing changed keeps the very array, so nothing redraws', () => {
  const have = [a, b];
  expect(patch(have, { rev: 2 })).toBe(have);
});

test('a changed card replaces its own place and keeps the others', () => {
  const b2 = { id: 'b', v: 2 };
  const out = patch([a, b, c], { rev: 2, changed: [b2] })!;
  expect(out).toEqual([a, b2, c]);
  expect(out[0]).toBe(a);
});

test('the order says who is left and where', () => {
  expect(patch([a, b, c], { rev: 2, order: ['c', 'a'] })).toEqual([c, a]);
  expect(patch([a, b], { rev: 2, order: ['b', 'c', 'a'], changed: [c] })).toEqual([b, c, a]);
  expect(patch([a], { rev: 2, order: [] })).toEqual([]);
});

test('a card it cannot find means out of step, not a hole in the list', () => {
  expect(patch([a], { rev: 2, order: ['a', 'x'] })).toBeNull();
});

test('events wait the merge window, then for the gap since the last refresh', () => {
  expect(refreshWait(10_000, 0)).toBe(MERGE_MS);
  expect(refreshWait(10_000, 10_000)).toBe(GAP_MS);
  expect(refreshWait(10_000, 10_000 - GAP_MS)).toBe(MERGE_MS);
  // The state change a person waits on still shows within a second of the event.
  expect(GAP_MS + 300).toBeLessThanOrEqual(1000);
});

/** A machine that answers `cards` or only `list`, recording what it was asked. */
class Fake {
  ended: any = null;
  asked: any[] = [];
  onEvent: ((m: any) => void) | null = null;
  replies: any[] = [];
  held: (() => void) | null = null;
  hold = false;
  constructor(readonly ops: string[]) {}
  async request(op: string, fields: any = {}) {
    if (op === 'hello') return { role: 'full', ops: this.ops, machine: { name: 'mac' }, os: 'macos' };
    if (op === 'ping') return {};
    this.asked.push({ op, ...fields });
    if (this.hold) await new Promise<void>((go) => (this.held = go));
    if (op === 'list') return { terminals: [a, b], spaces: [{ id: 's', name: 'S', cwd: '/s' }], todos: [] };
    return this.replies.shift() ?? { rev: (fields.since ?? 0) + 1 };
  }
  async stream(_: string, __: any, h: (m: any) => void) {
    this.onEvent = h;
    return { stream: 1, close: async () => undefined };
  }
  onEnded() {}
  close() {}
}
const sleep = (ms: number) => new Promise((go) => setTimeout(go, ms));
let fake: Fake;
const link = { machine: 'm', name: 'mac' } as any;
const machine = () => hub.getMachine('m')!;
async function up(ops: string[]) {
  fake = new Fake(ops);
  hub.stage([link], async () => fake as any);
  await hub.start();
  await sleep(10);
}
beforeEach(() => hub.stage([], async () => null as any));

test('a computer without the op gets asked for the whole list, as before', async () => {
  await up(['list', 'send']);
  expect(fake.asked).toEqual([{ op: 'list' }]);
  expect(machine().cards).toEqual([a, b] as any);
  expect(machine().spaces).toEqual({ s: 'S' });
  await hub.refresh('m');
  expect(fake.asked.map((x) => x.op)).toEqual(['list', 'list']);
});

test('the first ask has no version and each later one the last it was given', async () => {
  fake = new Fake(['cards']);
  fake.replies = [
    { rev: 1, order: ['a', 'b'], changed: [a, b], spaces: [{ id: 's', name: 'S', cwd: '/s' }], todos: [{ id: 't' }], machine: { name: 'new' } },
    { rev: 2, changed: [{ id: 'b', v: 2 }] },
    { rev: 3 },
  ];
  hub.stage([link], async () => fake as any);
  await hub.start();
  await sleep(10);
  expect(fake.asked).toEqual([{ op: 'cards' }]);
  expect(machine().cards).toEqual([a, b] as any);
  expect(machine().name).toBe('new');
  await hub.refresh('m');
  expect(fake.asked[1]).toEqual({ op: 'cards', since: 1 });
  expect(machine().cards[1]).toEqual({ id: 'b', v: 2 } as any);
  // What it did not say stays: spaces, todos and the name.
  expect(machine().spaces).toEqual({ s: 'S' });
  expect(machine().spaceDirs).toEqual({ '/s': 'S' });
  expect(machine().todos).toEqual([{ id: 't' }] as any);
  const cards = machine().cards;
  await hub.refresh('m');
  expect(fake.asked[2]).toEqual({ op: 'cards', since: 2 });
  expect(machine().cards).toBe(cards);
  expect(machine().name).toBe('new');
});

test('out of step, it asks again from nothing', async () => {
  fake = new Fake(['cards']);
  fake.replies = [{ rev: 1, order: ['a'], changed: [a] }, { rev: 2, order: ['a', 'ghost'] }, { rev: 3, order: ['a', 'b'], changed: [a, b] }];
  hub.stage([link], async () => fake as any);
  await hub.start();
  await sleep(10);
  await hub.refresh('m');
  expect(fake.asked.slice(1)).toEqual([{ op: 'cards', since: 1 }, { op: 'cards' }]);
  expect(machine().cards).toEqual([a, b] as any);
});

test('a refresh waits for the one in flight, and many waiting make one more', async () => {
  await up(['cards']);
  const before = fake.asked.length;
  fake.hold = true;
  const first = hub.refresh('m');
  await sleep(1);
  const more = [hub.refresh('m'), hub.refresh('m'), hub.refresh('m')];
  expect(fake.asked.length).toBe(before + 1);
  fake.hold = false;
  fake.held!();
  await first;
  await Promise.all(more);
  expect(fake.asked.length).toBe(before + 2);
  // The second knows what the first was told.
  expect(fake.asked.at(-1).since).toBe(fake.asked.at(-2).since + 1);
});

test('a machine busy all the time refreshes once a gap, not once an event', async () => {
  await up(['cards']);
  const before = fake.asked.length;
  const started = Date.now();
  while (Date.now() - started < GAP_MS * 2.5) {
    fake.onEvent!({ type: 'event', event: {} });
    await sleep(10);
  }
  await sleep(MERGE_MS + GAP_MS);
  const asks = fake.asked.length - before;
  expect(asks).toBeGreaterThanOrEqual(2);
  expect(asks).toBeLessThanOrEqual(5);
});

test('one event alone still refreshes after the merge window', async () => {
  await up(['cards']);
  const before = fake.asked.length;
  fake.onEvent!({ type: 'event', event: {} });
  await sleep(MERGE_MS + 40);
  expect(fake.asked.length).toBe(before + 1);
});
