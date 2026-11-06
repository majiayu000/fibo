import { mock } from 'bun:test';
type Choice = { lock: boolean; confirmDanger?: boolean };
let finish!: (value: Choice) => void;
let reading: Promise<Choice> = new Promise(resolve => { finish = resolve; });
let calls = 0, writes = 0, mode = 'success';
const sent: unknown[] = [];
mock.module('react-native', () => ({ Platform: { OS: 'ios' }, Alert: { alert() {} } }));
mock.module('expo-local-authentication', () => ({
  hasHardwareAsync: async () => false,
  authenticateAsync: async () => { calls++; if (mode === 'throw') throw new Error('not available'); return { success: mode === 'success' }; }
}));
mock.module('../../src/store/settings', () => ({ readSettings: () => reading }));
mock.module('../../src/remote/hub', () => ({ refresh() {}, write: async (_machine: string, _op: string, fields: unknown) => { writes++; sent.push(fields); }, NOT_YET: '' }));
const { verify, approve, sendToConversation } = await import('../../src/actions');
let resolved = false;
const pending = verify('open').then(ok => { resolved = true; return ok; });
await Bun.sleep(20);
const beforeRead = { resolved, calls };
finish({ lock: true, confirmDanger: false });
const afterRead = await pending;
mode = 'cancel'; const cancelled = await verify('open');
mode = 'throw'; const unavailable = await verify('open');
const approved = await approve('machine', { id: 'terminal' } as any, { known: true, subject: 'test' } as any);
reading = Promise.reject(new Error('Keychain unavailable'));
const unreadable = await verify('open');
reading = Promise.resolve({ lock: false });
const unlocked = await verify('open');
const authWrites = writes;
const shown = { id: 'terminal', session: 'original-session' };
const sending = sendToConversation('machine', shown, 'feedback');
shown.session = 'new-session';
await sending;
await sendToConversation('machine', { id: 'terminal' }, 'unidentified');
const result = JSON.stringify({ beforeRead, afterRead, cancelled, unavailable, approved, writes: authWrites, unreadable, unlocked, sent });
if (process.argv[2]) await Bun.write(process.argv[2], result);
else console.log(result);
