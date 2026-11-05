// A machine that answers `hello` with an error is shown by what it said, not as a
// network that is down.
import { describe, expect, mock, test } from 'bun:test';
import { RemoteError } from '../src/remote/client';
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

/** A connection whose machine refuses `hello`; closing it ends it with 1000, as a socket does. */
class Refusing {
  ended: { code: number; reason: string } | null = null;
  private handlers: ((e: { code: number; reason: string }) => void)[] = [];
  request(op: string): Promise<any> {
    if (op === 'hello') return Promise.reject(new RemoteError('unsupported version 1'));
    return Promise.resolve({});
  }
  onEnded(h: (e: { code: number; reason: string }) => void) {
    this.handlers.push(h);
  }
  close() {
    this.ended = { code: 1000, reason: '' };
    this.handlers.forEach((h) => h(this.ended!));
  }
}

const link: Link = { relay: 'wss://relay.test', code: new Uint8Array(32), device: 'phone', key: new Uint8Array(32), machine: 'm', name: 'm' };
hub.stage([link], async () => new Refusing() as any);

describe('a refused hello', () => {
  test("shows the machine's reason", async () => {
    await hub.start();
    await new Promise((done) => setTimeout(done, 0));
    const m = hub.getMachine('m');
    expect(m?.status).toBe('offline');
    expect(m?.error).toContain('unsupported version 1');
    hub.forget('m');
  });
});
