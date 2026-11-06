import { expect, test } from 'bun:test';
import { execFileSync } from 'node:child_process';
import { readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

test('authentication waits for settings and rejects cancellation and native errors', () => {
  // Native mocks stay isolated from the connection tests' real hub.
  const output = join(tmpdir(), `mooring-auth-${crypto.randomUUID()}.json`);
  execFileSync(process.execPath, [new URL('./native/auth.ts', import.meta.url).pathname, output], {
    cwd: new URL('..', import.meta.url).pathname, encoding: 'utf8', timeout: 5000,
  });
  const result = JSON.parse(readFileSync(output, 'utf8'));
  rmSync(output);
  expect(result).toEqual({
    beforeRead: { resolved: false, calls: 0 }, afterRead: true,
    cancelled: false, unavailable: false, approved: false, writes: 0,
    unreadable: false, unlocked: true,
    sent: [
      { terminal: 'terminal', session: 'original-session', text: 'feedback', enter: true },
      { terminal: 'terminal', session: '', text: 'unidentified', enter: true },
    ],
  });
});
