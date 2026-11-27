import { expect, test } from 'bun:test';
import { execFileSync } from 'node:child_process';
import { readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

test('approving asks no fingerprint, even on a phone saved with the old lock on; answers stay single', () => {
  // Native mocks stay isolated from the connection tests' real hub.
  const output = join(tmpdir(), `mooring-auth-${crypto.randomUUID()}.json`);
  execFileSync(process.execPath, [new URL('./native/auth.ts', import.meta.url).pathname, output], {
    cwd: new URL('..', import.meta.url).pathname, encoding: 'utf8', timeout: 5000,
  });
  const result = JSON.parse(readFileSync(output, 'utf8'));
  rmSync(output);
  expect(result).toEqual({
    oldLock: { approved: true, writes: 1 },
    answers: { duplicate: false, duplicateTrust: false, whilePending: 1, afterRetry: 2 },
    denial: { retried: true, told: true, writes: [
      { terminal: 'terminal', question: 'question', allow: false, subject: 'test' },
      { terminal: 'terminal', session: 'denied-session', text: 'try a different approach', enter: true },
    ] },
    sent: [
      { terminal: 'terminal', session: 'original-session', text: 'feedback', enter: true },
      { terminal: 'terminal', session: '', text: 'unidentified', enter: true },
    ],
  });
});
