import { beforeEach, afterEach, expect, mock, spyOn, test } from 'bun:test';
import { rn } from './rn';
mock.module('react-native', () => rn());
mock.module('expo-secure-store', () => ({ getItemAsync: async () => null, setItemAsync: async () => {}, deleteItemAsync: async () => {} }));
const { kv, persisted } = await import('../src/store/kv');
const machines = await import('../src/store/machines');
const link = { machine: 'stored', name: 'Mac', device: 'phone', relay: 'wss://example.test', code: new Uint8Array(32), key: new Uint8Array(32) };

beforeEach(() => { spyOn(globalThis, 'alert').mockImplementation(() => {}); });
afterEach(() => mock.restore());
function storage(value: string | null) {
  let text = value;
  const get = spyOn(kv, 'get').mockImplementation(async () => text);
  const set = spyOn(kv, 'set').mockImplementation(async (_k, v) => { text = v; });
  const del = spyOn(kv, 'del').mockImplementation(async () => { text = null; });
  return { get, set, del, text: () => text };
}

test('broken JSON survives load followed by set byte for byte', async () => {
  const s = storage('{broken');
  const p = persisted('broken', { n: 0 });
  await p.ready;
  expect(await p.set({ n: 1 })).toBe(false);
  expect(s.text()).toBe('{broken');
  expect(s.set).not.toHaveBeenCalled();
});

test('a failed read never turns into permission to overwrite', async () => {
  const s = storage('{"n":9}');
  s.get.mockRejectedValue(new Error('locked'));
  const p = persisted('locked', { n: 0 });
  expect(await p.set({ n: 1 })).toBe(false);
  expect(s.text()).toBe('{"n":9}');
});

test('writes wait for startup, keep unknown fields, and skip unchanged JSON', async () => {
  const s = storage('{"n":4,"future":"kept"}');
  const p = persisted('valid', { n: 0 });
  expect(await p.set(v => ({ ...v, n: v.n + 1 }))).toBe(true);
  expect(JSON.parse(s.text()!)).toEqual({ n: 5, future: 'kept' });
  await p.set({ n: 5 });
  await p.set(v => v);
  expect(s.set).toHaveBeenCalledTimes(1);
});

test('a failed save does not claim the new value and a later save can retry', async () => {
  const s = storage('{"n":4}');
  const p = persisted('full', { n: 0 });
  await p.ready;
  s.set.mockRejectedValueOnce(new Error('full'));
  expect(await p.set({ n: 5 })).toBe(false);
  expect(p.get().n).toBe(4);
  expect(await p.set({ n: 6 })).toBe(true);
  expect(p.get().n).toBe(6);
});

test('a broken machine index prevents both pairing and forgetting before any mutation', async () => {
  const s = storage('{broken');
  await expect(machines.saveMachine(link)).rejects.toThrow();
  await expect(machines.forgetMachine('stored')).rejects.toThrow();
  expect(s.set).not.toHaveBeenCalled();
  expect(s.del).not.toHaveBeenCalled();
  expect(s.text()).toBe('{broken');
});
