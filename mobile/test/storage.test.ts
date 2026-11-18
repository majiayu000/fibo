import { afterEach, expect, mock, spyOn, test } from 'bun:test';
import { execFileSync } from 'node:child_process';
import { readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rn } from './rn';
mock.module('react-native', () => rn());
mock.module('expo-secure-store', () => ({ getItemAsync: async () => null, setItemAsync: async () => {}, deleteItemAsync: async () => {} }));
const { kv, persisted } = await import('../src/store/kv');

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
  await expect(p.ready).rejects.toThrow();
  await expect(p.set({ n: 1 })).rejects.toThrow();
  expect(s.text()).toBe('{broken');
  expect(s.set).not.toHaveBeenCalled();
});

test('a failed read never turns into permission to overwrite', async () => {
  const s = storage('{"n":9}');
  s.get.mockRejectedValue(new Error('locked'));
  const p = persisted('locked', { n: 0 });
  await expect(p.set({ n: 1 })).rejects.toThrow('locked');
  expect(s.text()).toBe('{"n":9}');
});

test('writes wait for startup, keep unknown fields, and skip unchanged JSON', async () => {
  const s = storage('{"n":4,"future":"kept"}');
  const p = persisted('valid', { n: 0 });
  await p.set(v => ({ ...v, n: v.n + 1 }));
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
  await expect(p.set({ n: 5 })).rejects.toThrow('full');
  expect(p.get().n).toBe(4);
  await p.set({ n: 6 });
  expect(p.get().n).toBe(6);
});

test('a broken machine index prevents both pairing and forgetting before any mutation', () => {
  // hub.test.ts mocks machines for the whole Bun process; ask the real store in a fresh one.
  const output = join(tmpdir(), `fibo-machines-${crypto.randomUUID()}.json`);
  try {
    execFileSync(process.execPath, [new URL('./native/machines.ts', import.meta.url).pathname, output], {
      cwd: new URL('..', import.meta.url).pathname, encoding: 'utf8', timeout: 5000,
    });
    expect(JSON.parse(readFileSync(output, 'utf8'))).toEqual({
      saveRejected: true, forgetRejected: true, writes: 0, deletes: 0, index: '{broken', machine: 'kept',
    });
  } finally {
    rmSync(output, { force: true });
  }
});
