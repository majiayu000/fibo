// Keep the real machine store separate from hub.test.ts's process-wide module mock.
import { mock } from 'bun:test';
import { rn } from '../rn';
mock.module('react-native', () => rn());
mock.module('expo-secure-store', () => ({}));
const saved = new Map([
  ['mooring.machines', '{broken'],
  ['mooring.machine.stored', 'kept'],
]);
let writes = 0, deletes = 0;
(globalThis as { localStorage?: unknown }).localStorage = {
  getItem: (key: string) => saved.get(key) ?? null,
  setItem(key: string, value: string) { writes++; saved.set(key, value); },
  removeItem(key: string) { deletes++; saved.delete(key); },
};
const machines = await import('../../src/store/machines');
const link = { machine: 'stored', name: 'Mac', device: 'phone', relay: 'wss://example.test', code: new Uint8Array(32), key: new Uint8Array(32) };
const saveRejected = await machines.saveMachine(link).then(() => false, () => true);
const forgetRejected = await machines.forgetMachine('stored').then(() => false, () => true);
const result = JSON.stringify({
  saveRejected, forgetRejected, writes, deletes,
  index: saved.get('mooring.machines'), machine: saved.get('mooring.machine.stored'),
});
if (process.argv[2]) await Bun.write(process.argv[2], result);
else console.log(result);
