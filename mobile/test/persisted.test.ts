import { expect, test } from 'bun:test';
import { persistedValue } from '../src/store/persisted';

function disk(text: string | null = null) {
  return {
    text,
    failRead: false,
    failWrite: false,
    async get() { if (this.failRead) throw new Error('Keychain unavailable'); return this.text; },
    async set(_key: string, value: string) { if (this.failWrite) throw new Error('Keychain write rejected'); this.text = value; },
  };
}

test('rejected outbox writes keep the previously saved queue and report failure', async () => {
  const storage = disk();
  const box = persistedValue(storage, 'outbox', { items: [] as string[] });
  await box.ready;
  storage.failWrite = true;
  await expect(box.set({ items: ['unsaved message'] })).rejects.toThrow('write rejected');
  expect(box.get().items).toEqual([]);
  expect(storage.text).toBeNull();
  storage.failWrite = false;
  await box.set({ items: ['retry'] });
  const restarted = persistedValue(storage, 'outbox', { items: [] as string[] });
  await restarted.ready;
  expect(restarted.get().items).toEqual(['retry']);
});

test('read failures block writes until an explicit successful reload', async () => {
  const storage = disk(JSON.stringify({ lock: true }));
  storage.failRead = true;
  const settings = persistedValue(storage, 'settings', { lock: false });
  await expect(settings.ready).rejects.toThrow('unavailable');
  await expect(settings.set({ lock: false })).rejects.toThrow('unavailable');
  expect(storage.text).toBe('{"lock":true}');
  storage.failRead = false;
  await settings.reload();
  expect(settings.get().lock).toBe(true);
});

test('malformed stored settings cannot be read as unlocked defaults', async () => {
  for (const text of ['{broken', 'null', 'false', '[]']) {
    const settings = persistedValue(disk(text), 'settings', { lock: false });
    await expect(settings.ready).rejects.toThrow();
  }
});

test('concurrent edits made during hydration preserve saved and new messages in order', async () => {
  const storage = disk('{"items":["saved"]}');
  let release!: () => void;
  const held = new Promise<void>((done) => { release = done; });
  const box = persistedValue({ get: async () => { await held; return storage.text; }, set: storage.set.bind(storage) }, 'outbox', { items: [] as string[] });
  const first = box.set((v) => ({ items: [...v.items, 'first'] }));
  const second = box.set((v) => ({ items: [...v.items, 'second'] }));
  release();
  await Promise.all([first, second]);
  expect(JSON.parse(storage.text!).items).toEqual(['saved', 'first', 'second']);
  expect(box.get().items).toEqual(['saved', 'first', 'second']);
});

test('failed queue removal retains the same request id after restart', async () => {
  const storage = disk('{"items":[{"id":"original-request","session":"original-session"}]}');
  const box = persistedValue(storage, 'outbox', { items: [] as { id: string; session: string }[] });
  await box.ready;
  storage.failWrite = true;
  await expect(box.set({ items: [] })).rejects.toThrow();
  const restarted = persistedValue(storage, 'outbox', { items: [] as { id: string; session: string }[] });
  await restarted.ready;
  expect(restarted.get().items).toEqual([{ id: 'original-request', session: 'original-session' }]);
});

test('reloading behind a queued write does not deadlock or revert it', async () => {
  const storage = disk();
  const value = persistedValue(storage, 'settings', { lock: false });
  const writing = value.set({ lock: true });
  await Promise.all([writing, value.reload()]);
  expect(value.get().lock).toBe(true);
});

test('a value an older build saved reads its missing fields as their defaults', async () => {
  const storage = disk(JSON.stringify({ lock: true }));
  const settings = persistedValue(storage, 'settings', { lock: false, language: 'auto' });
  await settings.ready;
  expect(settings.get()).toEqual({ lock: true, language: 'auto' });
});
