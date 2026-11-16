// The real quota store over phone storage whose first read fails: run in its own process by
// test/quota-cache.test.ts, since other files mock `store/quota` for the whole run.
import { mock } from 'bun:test';
mock.module('react-native', () => ({ Platform: { OS: 'web' }, Alert: { alert() {} } }));
mock.module('expo-secure-store', () => ({}));
const saved = new Map<string, string>();
let failRead = true;
(globalThis as { localStorage?: unknown }).localStorage = {
  getItem(k: string) {
    if (failRead) throw new Error('Keychain unavailable');
    return saved.get(k) ?? null;
  },
  setItem(k: string, v: string) { saved.set(k, v); },
  removeItem(k: string) { saved.delete(k); },
};
const { quotaStore } = await import('../../src/store/quota');
const store = quotaStore('mac');
const hydrated = await store.ready.then(() => true, () => false);
failRead = false;
const fresh = { agent: 'claude', at: 1, windows: [] };
const wrote = await store.set({ agents: [fresh] }).then(() => true, () => false);
const result = JSON.stringify({ hydrated, wrote, shown: store.get().agents?.length ?? null, saved: saved.size });
if (process.argv[2]) await Bun.write(process.argv[2], result);
else console.log(result);
