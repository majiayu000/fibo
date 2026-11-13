// Close codes and limits agree with the machine's (`vectors/v1.json`), and each code
// leads the phone to do the right thing.
import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { Close, closeOutcome } from '../src/remote/close';
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
