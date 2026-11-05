// One connection to a machine, whatever calls for it. On a real phone `mooring remote --status`
// showed three for one device and its log bursts of twenty: a dial in flight was invisible to
// the guard, and a timer forgotten by `retry.delete` fired anyway, so each lost connection came
// back as many.
import { afterEach, beforeEach, expect, mock, test } from 'bun:test';

let onAppState: ((s: string) => void) | null = null;
mock.module('react-native', () => ({
  AppState: {
    addEventListener: (_: string, f: (s: string) => void) => {
      onAppState = f;
      return { remove() {} };
    },
  },
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

/** A connection a fake relay hands out; `end` is its socket closing. */
class Fake {
  ended: any = null;
  closed = false;
  handlers: ((e: any) => void)[] = [];
  async request(op: string) {
    if (this.ended) throw new Error('gone');
    if (op === 'hello') return { role: 'full', ops: ['send'], machine: { name: 'mac' }, os: 'macos' };
    if (op === 'list') return { terminals: [], spaces: [] };
    return {};
  }
  async stream() {
    return { stream: 1, close: async () => undefined };
  }
  onEnded(h: (e: any) => void) {
    if (this.ended) h(this.ended);
    else this.handlers.push(h);
  }
  close() {
    this.closed = true;
    this.end({ code: 1000, reason: '' });
  }
  end(e: any) {
    if (this.ended) return;
    this.ended = e;
    for (const h of this.handlers) h(e);
  }
}

/** Every dial goes through here: held until released, like a relay waiting on its machine. */
let dials: { link: any; resolve: (r: Fake) => void; reject: (e: any) => void; socket?: Fake }[] = [];
const live = () => dials.filter((d) => d.socket && !d.socket.ended).length;
const settle = () => new Promise((done) => setTimeout(done, 5));
async function answerAll() {
  for (const d of dials.filter((d) => !d.socket)) {
    d.socket = new Fake();
    d.resolve(d.socket);
  }
  await settle();
}
async function failAll() {
  for (const d of dials.filter((d) => !d.socket)) {
    d.socket = new Fake();
    d.socket.ended = {};
    d.reject(Object.assign(new Error('down'), { code: 1012 }));
  }
  await settle();
}

const LINK = { machine: 'm1', name: 'mac', device: 'phone', relay: 'ws://x', code: 'c', key: 'k' } as any;
const started = (async () => {
  hub.stage([LINK], (link: any) => new Promise((resolve, reject) => dials.push({ link, resolve: resolve as any, reject })));
  await hub.start();
})();

beforeEach(async () => {
  await started;
  await settle();
  // Whatever the last test left: a clean, online, single connection, no timers.
  hub.reconnect('m1');
  await answerAll();
  for (const d of dials) d.socket?.end({ code: 1012, reason: '' });
  await settle();
  hub.reconnect('m1');
  dials = dials.filter((d) => !d.socket);
  await answerAll();
  await settle();
});
afterEach(() => {
  for (const d of dials) d.socket?.close();
});

test('coming back, retrying and a timer while a dial is on its way dial once', async () => {
  const before = dials.length;
  dials[dials.length - 1].socket!.end({ code: 1012, reason: 'machine left' });
  await settle();
  // Backing off now. Foreground and 重试 both call while the first dial is out.
  onAppState!('active');
  await settle();
  hub.reconnect('m1');
  onAppState!('active');
  await settle();
  expect(dials.length).toBe(before + 1);
  await answerAll();
  expect(live()).toBe(1);
  expect(hub.getMachine('m1')?.status).toBe('online');
});

test('a backoff timer replaced by a foreground dial does not fire later', async () => {
  const before = dials.length;
  dials[dials.length - 1].socket!.end({ code: 1012, reason: '' });
  await settle();
  // The 1-second timer is armed; coming back dials now and must cancel it.
  onAppState!('active');
  await settle();
  await answerAll();
  expect(dials.length).toBe(before + 1);
  await new Promise((done) => setTimeout(done, 1100));
  expect(dials.length).toBe(before + 1);
  expect(live()).toBe(1);
});

test('failed dials leave one timer, so a lost connection stays one', async () => {
  const count = () => dials.length;
  dials[dials.length - 1].socket!.end({ code: 1012, reason: '' });
  await settle();
  onAppState!('active');
  await settle();
  const after = count();
  await failAll();
  await new Promise((done) => setTimeout(done, 1100));
  expect(count()).toBe(after + 1);
  await answerAll();
  expect(live()).toBe(1);
});

test('the old connection ending says nothing about the new one', async () => {
  const old = dials[dials.length - 1].socket!;
  const paired = hub.pair(LINK);
  await settle();
  await answerAll(); // its handshake check
  await answerAll(); // the connection itself
  await paired;
  await answerAll();
  old.end({ code: 1012, reason: '' });
  await settle();
  expect(hub.getMachine('m1')?.status).toBe('online');
  expect(hub.remote('m1')).not.toBeNull();
  // The handshake check closed itself; the old one ended on its own; one is left.
  expect(live()).toBe(1);
});

// Pairing again against that one connection: a replaced connection ends without taking its
// successor down, and a dial for the old link is dropped.
const answer = (at: number) => {
  const d = dials[at];
  d.socket = new Fake();
  d.resolve(d.socket);
  return d.socket;
};
const linkFor = (machine: string, device = 'phone') => ({ ...LINK, machine, name: machine, device });
/** Pairs, answering the probe and then the connection; gives the connection. */
async function paired(link: any) {
  const from = dials.length;
  const done = hub.pair(link);
  await settle();
  answer(from);
  await settle();
  const socket = answer(from + 1);
  await done;
  await settle();
  return socket;
}

test('tapping retry while it dials does not dial again', async () => {
  const socket = await paired(linkFor('m4'));
  socket.end({ code: 4401, reason: '' });
  await settle();
  const before = dials.length;
  hub.reconnect('m4');
  hub.reconnect('m4');
  hub.reconnect('m4');
  expect(dials.length).toBe(before + 1);
  answer(before);
  await settle();
  expect(hub.getMachine('m4')?.status).toBe('online');
});

test('the old connection ending after a new pairing leaves the new one', async () => {
  const old = await paired(linkFor('m2'));
  const fresh = await paired(linkFor('m2'));
  expect(old.closed).toBe(true);
  old.end({ code: 1000, reason: '' });
  await settle();
  expect(hub.remote('m2')).toBe(fresh as any);
  expect(hub.getMachine('m2')?.status).toBe('online');
});

test('pairing again mid-dial drops that dial and dials the new link', async () => {
  const socket = await paired(linkFor('m3'));
  socket.end({ code: 4401, reason: '' });
  await settle();
  const pending = dials.length;
  hub.reconnect('m3');
  const again = linkFor('m3', 'phone-2');
  const done = hub.pair(again);
  await settle();
  answer(pending + 1);
  await settle();
  // The old dial answers late: nobody wants it. The new link's dial is already out.
  const late = answer(pending);
  await settle();
  expect(late.closed).toBe(true);
  expect(dials.at(-1)?.link).toBe(again);
  const now = answer(dials.length - 1);
  await done;
  await settle();
  expect(hub.remote('m3')).toBe(now as any);
  expect(hub.getMachine('m3')?.status).toBe('online');
});
