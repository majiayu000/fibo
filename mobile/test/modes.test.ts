// The mode that stops asking: a second confirmation and Face ID first, and only then the field
// the computer wants. Anything else that goes through here cannot carry it.
import { beforeEach, expect, mock, test } from 'bun:test';
import { rn } from './rn';

const sent: any[] = [];
let answer = true;
let lock = false;
let passes = true;
const asked: string[] = [];
mock.module('react-native', () =>
  rn({
    Platform: { OS: 'ios' },
    Alert: {
    alert: (title: string, detail: string, buttons: { text: string; onPress?: () => void }[]) => {
      asked.push(`${title}\n${detail}`);
      buttons[answer ? 1 : 0].onPress?.();
    },
    },
  }),
);
mock.module('expo-local-authentication', () => ({
  hasHardwareAsync: async () => true,
  authenticateAsync: async () => ({ success: passes }),
}));
mock.module('../src/store/machines', () => ({
  loadMachines: async () => [],
  saveMachine: async () => undefined,
  forgetMachine: async () => undefined,
  machineId: (l: any) => l.machine,
}));
mock.module('../src/store/quota', () => ({ forgetQuota: async () => undefined }));
mock.module('../src/store/settings', () => ({ settings: { get: () => ({ lock, confirmDanger: true }) } }));
const hub = await import('../src/remote/hub');
const { switchMode } = await import('../src/actions');
const { FULL_CONFIRM, FULL_WORDS, hasEffort, isFull, kindName, rowSaysFull } = await import('../src/ui/modes');

const FULL = { key: 'bypassPermissions', label: FULL_WORDS, full: true };
const PLAIN = { key: 'plan', label: '只做计划' };

/** A machine that keeps what it was asked; `named` adds a request id each write. */
class Fake {
  ended = null;
  async request(op: string, fields: any = {}) {
    if (op === 'hello') return { role: 'full', ops: ['setmode'], machine: { name: 'mac' }, os: 'macos' };
    if (op === 'setmode') sent.push({ op, ...fields });
    return { terminals: [], spaces: [] };
  }
  async stream() {
    return { stream: 1, close: async () => undefined };
  }
  onEnded() {}
  close() {}
}

beforeEach(async () => {
  sent.length = 0;
  asked.length = 0;
  answer = true;
  passes = true;
  lock = false;
  hub.stage([{ machine: 'modes-m', name: 'mac' } as any], async () => new Fake() as any);
  await hub.start();
  await new Promise((go) => setTimeout(go, 5));
});

const plain = () => sent.map(({ request, ...rest }) => rest);

test('an ordinary mode goes at once and carries nothing extra', async () => {
  expect(await switchMode('modes-m', 't', 'permission', PLAIN)).toBe(true);
  expect(asked).toEqual([]);
  expect(plain()).toEqual([{ op: 'setmode', terminal: 't', kind: 'permission', choice: 'plan' }]);
});

test('a model never asks and never carries it', async () => {
  await switchMode('modes-m', 't', 'model', { key: 'Sonnet 5.5', label: 'Sonnet 5.5' });
  expect(asked).toEqual([]);
  expect(sent[0].allow_full).toBeUndefined();
});

test('the full mode says what it means first, and no is no', async () => {
  answer = false;
  expect(await switchMode('modes-m', 't', 'permission', FULL)).toBe(false);
  expect(sent).toEqual([]);
  expect(asked[0]).toContain('不再问你');
  expect(asked[0]).toContain(FULL_CONFIRM.title);
});

test('with the lock on, a failed Face ID sends nothing', async () => {
  lock = true;
  passes = false;
  expect(await switchMode('modes-m', 't', 'permission', FULL)).toBe(false);
  expect(sent).toEqual([]);
});

test('confirmed, and through Face ID if on, it carries allow_full', async () => {
  lock = true;
  expect(await switchMode('modes-m', 't', 'permission', FULL)).toBe(true);
  expect(sent[0].allow_full).toBe(true);
  expect(sent[0].choice).toBe('bypassPermissions');
});

test('the word on a card is what marks it', () => {
  expect(isFull(FULL_WORDS)).toBe(true);
  expect(isFull('自动判断')).toBe(false);
  expect(isFull(null)).toBe(false);
});

test('a row says it only when it is the exception', () => {
  const full = { permission: FULL_WORDS };
  const asks = { permission: '逐个确认' };
  expect(rowSaysFull(full, [full, full])).toBe(false);
  expect(rowSaysFull(full, [full, asks])).toBe(true);
  expect(rowSaysFull(full, [full, { permission: null }])).toBe(false);
  expect(rowSaysFull(asks, [full, asks])).toBe(false);
});

test('an effort goes as its own kind, and only claude and codex offer it', async () => {
  expect(await switchMode('modes-m', 't', 'effort', { key: 'xhigh', label: 'xhigh' })).toBe(true);
  expect(asked).toEqual([]);
  expect(plain()).toEqual([{ op: 'setmode', terminal: 't', kind: 'effort', choice: 'xhigh' }]);
  expect(kindName('effort')).toBe('强度');
  expect([hasEffort('claude'), hasEffort('codex'), hasEffort('grok'), hasEffort(undefined)]).toEqual([true, true, false, false]);
});
