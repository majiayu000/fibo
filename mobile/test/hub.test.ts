// The hub keeps one connection per machine: a second dial waits for the first, and a
// connection that was replaced ends without taking its successor down.
import { afterAll, describe, expect, mock, test } from 'bun:test';
import type { Link } from '../src/remote/link';

mock.module('react-native', () => ({ AppState: { addEventListener: () => ({ remove() {} }) } }));
mock.module('../src/store/machines', () => ({
  machineId: (l: Link) => l.machine || `device-${l.device}`,
  loadMachines: async () => [],
  saveMachine: async () => undefined,
  forgetMachine: async () => undefined,
}));
mock.module('../src/store/quota', () => ({ forgetQuota: async () => undefined }));

const hub = await import('../src/remote/hub');

/** A connection that answers every request and ends only when told. */
class Fake {
  ended: { code: number; reason: string } | null = null;
  closed = false;
  private handlers: ((e: { code: number; reason: string }) => void)[] = [];
  request(op: string): Promise<any> {
    if (this.ended) return Promise.reject(new Error('gone'));
    return Promise.resolve(op === 'hello' ? { role: 'full', ops: [] } : op === 'events' ? { stream: 1 } : { terminals: [] });
  }
  async stream(op: string) {
    const ok = await this.request(op);
    return { stream: ok.stream, close: async () => undefined };
  }
  onEnded(h: (e: { code: number; reason: string }) => void) {
    if (this.ended) h(this.ended);
    else this.handlers.push(h);
  }
  close() {
    this.closed = true;
  }
  end(code: number) {
    this.ended = { code, reason: '' };
    this.handlers.forEach((h) => h(this.ended!));
  }
}

/** Dials that answer when the test says. */
const dials: { link: Link; answer: (r: Fake) => void }[] = [];
const fakes: Fake[] = [];
const answer = (at: number) => {
  const f = new Fake();
  fakes.push(f);
  dials[at].answer(f);
  return f;
};
const tick = () => new Promise((done) => setTimeout(done, 0));
const link = (machine: string, device = 'phone'): Link => ({
  relay: 'wss://relay.test',
  code: new Uint8Array(32),
  device,
  key: new Uint8Array(32),
  machine,
  name: machine,
});

hub.stage([], (l) => new Promise((done) => dials.push({ link: l, answer: done as (r: Fake) => void })) as any);
await hub.start();

afterAll(() => fakes.forEach((f) => f.ended || f.end(1000)));

/** Pairs `l`, answering the probe and then the connection; gives the connection. */
async function paired(l: Link) {
  const from = dials.length;
  const done = hub.pair(l);
  await tick();
  answer(from);
  await tick();
  const f = answer(from + 1);
  await done;
  await tick();
  return f;
}

describe('one connection per machine', () => {
  test('tapping retry while it dials does not dial again', async () => {
    const f = await paired(link('m1'));
    f.end(4401);
    await tick();
    const before = dials.length;
    hub.reconnect('m1');
    hub.reconnect('m1');
    hub.reconnect('m1');
    expect(dials.length).toBe(before + 1);
    answer(before);
    await tick();
    expect(hub.getMachine('m1')?.status).toBe('online');
  });

  test('the old connection ending after a new pairing leaves the new one', async () => {
    const old = await paired(link('m2'));
    const fresh = await paired(link('m2'));
    expect(old.closed).toBe(true);
    old.end(1000);
    await tick();
    expect(hub.remote('m2')).toBe(fresh as any);
    expect(hub.getMachine('m2')?.status).toBe('online');
  });

  test('pairing again mid-dial drops that dial and dials the new link', async () => {
    const f = await paired(link('m3'));
    f.end(4401);
    await tick();
    const pending = dials.length;
    hub.reconnect('m3');
    const again = link('m3', 'phone-2');
    const done = hub.pair(again);
    await tick();
    answer(pending + 1);
    await done;
    const late = answer(pending);
    await tick();
    expect(late.closed).toBe(true);
    expect(dials.at(-1)?.link).toBe(again);
    const now = answer(dials.length - 1);
    await tick();
    expect(hub.remote('m3')).toBe(now as any);
    expect(hub.getMachine('m3')?.status).toBe('online');
  });
});
