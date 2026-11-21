// Close codes and limits agree with the machine's (`vectors/v1.json`), and each code
// leads the phone to do the right thing.
import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { Close, closeOutcome, unreachedText, whatFailed } from '../src/remote/close';
import { LinkError, parseLink } from '../src/remote/link';
import { reachRelay } from '../src/remote/reach';
import { CHUNK } from '../src/remote/noise';

const v = JSON.parse(
  readFileSync(join(import.meta.dir, '../../crates/fibo-remote/vectors/v1.json'), 'utf8'),
);

describe('the same numbers as the machine', () => {
  test('close codes, by name', () => {
    const ours = Object.fromEntries(Object.entries(Close).map(([k, n]) => [k.toLowerCase(), n]));
    expect(ours).toEqual(v.close);
  });

  test('chunk size', () => {
    expect(CHUNK).toBe(v.limits.chunk);
  });
});

describe('what each close means here', () => {
  test('unpaired and a retired code are refused and never retried', () => {
    for (const code of [Close.UNPAIRED, Close.UNKNOWN_MACHINE]) {
      expect(closeOutcome(code)).toMatchObject({ status: 'refused', retry: 'never' });
    }
    expect(closeOutcome(Close.UNKNOWN_MACHINE).text).toContain('需要重新配对');
  });

  test('a disconnect from the computer is offline with the usual reconnect', () => {
    const o = closeOutcome(Close.KICKED);
    expect(o).toMatchObject({ status: 'offline', retry: 'later' });
    expect(o.text).toContain('断开');
  });

  test('a new role reconnects at once to get it', () => {
    expect(closeOutcome(Close.ROLE_CHANGED)).toMatchObject({ status: 'offline', retry: 'now' });
  });

  test('everything else, and no code at all, is offline and retried', () => {
    for (const code of [Close.NORMAL, Close.MACHINE_GONE, Close.TOO_SLOW, Close.BAD_MESSAGE, 1006, undefined]) {
      expect(closeOutcome(code)).toMatchObject({ status: 'offline', retry: 'later' });
    }
    expect(closeOutcome(1006).text).toContain('1006');
  });
});

describe('a dial with no close code says which end is away, as far as the relay can tell', () => {
  test('the relay answering its health check means the computer is the one away', async () => {
    const asked: string[] = [];
    const reach = await reachRelay('https://relay.example/base/', async (url) => (asked.push(url), { ok: true }));
    expect(asked).toEqual(['https://relay.example/base/healthz']);
    expect(reach).toBe('up');
    expect(unreachedText(reach)).toContain('电脑不在线');
  });

  test('no answer at all is the phone’s network or the relay, and says to check the network first', async () => {
    const reach = await reachRelay('wss://relay.example', async () => {
      throw new TypeError('Network request failed');
    });
    expect(reach).toBe('none');
    expect(unreachedText(reach)).toContain('手机有没有网');
  });

  test('a health check that hangs counts as no answer', async () => {
    const reach = await reachRelay('relay.example', (_, init) => new Promise((_, no) => init?.signal?.addEventListener('abort', () => no(new Error('aborted')))), 20);
    expect(reach).toBe('none');
  });

  test('a relay answering badly is not called the computer', async () => {
    expect(await reachRelay('https://relay.example', async () => ({ ok: false }))).toBe('sick');
    expect(unreachedText('sick')).not.toContain('电脑');
  });

  test('when nobody could ask, every possibility is named with a next step', () => {
    expect(unreachedText('unknown')).toBe(closeOutcome(undefined).text);
    expect(closeOutcome(undefined).text).toContain('先确认手机能上网');
  });
});

describe('a pairing link the phone cannot use says why in words and keeps the reason for 详情', () => {
  const failed = (text: string) => {
    try {
      parseLink(text);
    } catch (e) {
      return { ...whatFailed(e), kind: (e as LinkError).kind };
    }
    throw new Error('parsed');
  };

  test('another app’s link, an old version, a cut-off link', () => {
    const foreign = failed('https://example.com/pair');
    expect(foreign.kind).toBe('foreign');
    expect(foreign.text).toContain('配对新手机');
    expect(foreign.text).not.toContain('mooring://');
    expect(foreign.detail).toBe('not a mooring://pair link');
    const version = failed('mooring://pair?v=1&relay=x');
    expect(version.text).toContain('更新到最新');
    expect(version.detail).toContain('version 1');
    expect(failed('mooring://pair?v=2&relay=x').text).toContain('不完整');
  });
});
