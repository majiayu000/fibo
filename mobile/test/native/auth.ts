import { mock } from 'bun:test';
type Choice = { lock: boolean; confirmDanger?: boolean; lockWay?: 'strong' | 'weak' };
let finish!: (value: Choice) => void;
let reading: Promise<Choice> = new Promise(resolve => { finish = resolve; });
let calls = 0, writes = 0, mode = 'success';
const sent: unknown[] = [];
let writing: Promise<void> | null = null;
let finishWrite!: () => void;
const platform = { OS: 'ios', Version: '18.0' as number | string };
let enrolled = 0, enrollmentReads = 0;
const prompts: { biometricsSecurityLevel?: string; disableDeviceFallback?: boolean }[] = [];
mock.module('react-native', () => ({ Platform: platform, Alert: { alert() {} } }));
mock.module('expo-local-authentication', () => ({
  getEnrolledLevelAsync: async () => { enrollmentReads++; return enrolled; },
  supportedAuthenticationTypesAsync: async () => [1, 2],
  authenticateAsync: async (options: (typeof prompts)[number]) => {
    prompts.push(options);
    calls++;
    if (mode === 'throw') throw new Error('not available');
    return { success: mode === 'success' };
  }
}));
mock.module('../../src/store/settings', () => ({ readSettings: () => reading, useSettings: () => reading }));
mock.module('../../src/remote/hub', () => ({ getMachine: () => ({ status: 'online', cards: [{ id: 'terminal', session: 'denied-session' }] }), refresh() {}, write: async (_machine: string, _op: string, fields: unknown) => { writes++; sent.push(fields); if (writing) await writing; }, NOT_YET: '' }));
const { verify, verifyWhy, approve, deny, answerTrust, sendToConversation } = await import('../../src/actions');
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
const feedback = [...sent];
const beforeAnswers = writes;
writing = new Promise(resolve => { finishWrite = resolve; });
const card = { id: 'terminal', approval: 'question', trust: { folder: '/test' } } as any;
const ask = { known: true, subject: 'test' } as any;
const first = approve('machine', card, ask);
await Bun.sleep(10);
const second = deny('machine', card, ask, 'must not send');
const third = answerTrust('machine', card, true);
await Bun.sleep(10);
const whilePending = writes - beforeAnswers;
finishWrite(); await first;
const duplicate = await second;
const duplicateTrust = await third;
writing = null;
const retried = await deny('machine', card, ask);
const afterRetry = writes - beforeAnswers;
card.session = 'denied-session';
const telling = deny('machine', card, ask, 'try a different approach');
card.session = 'later-session';
const told = await telling;
const denial = { retried, told, writes: sent.slice(beforeAnswers + afterRetry) };
platform.OS = 'android';
mode = 'success';
enrolled = 3;
reading = Promise.resolve({ lock: true, lockWay: 'strong' });
const android: unknown[] = [];
for (const api of [27, 28, 29, 30]) {
  platform.Version = api;
  const ok = await verify('open');
  const prompt = prompts.at(-1)!;
  android.push({ api, ok, level: prompt.biometricsSecurityLevel, screenLock: !prompt.disableDeviceFallback });
}

// A foreground refresh and an enable tap share one query after enrollment changes.
const { refreshLockKit } = await import('../../src/ui/uselock');
platform.Version = 29;
const beforeEnrollment = enrollmentReads;
enrolled = 0;
const beforePin = (await refreshLockKit()).level;
enrolled = 1;
const foreground = refreshLockKit();
const fromSettings = refreshLockKit();
const afterPin = (await fromSettings).level;
const relocked = (await verifyWhy('foreground', foreground)).ok;
const enrollment = { beforePin, afterPin, relocked, shared: foreground === fromSettings, reads: enrollmentReads - beforeEnrollment };

const result = JSON.stringify({ android, enrollment, beforeRead, afterRead, cancelled, unavailable, approved, writes: authWrites, unreadable, unlocked, sent: feedback, answers: { duplicate, duplicateTrust, whilePending, afterRetry }, denial });
if (process.argv[2]) await Bun.write(process.argv[2], result);
else console.log(result);
