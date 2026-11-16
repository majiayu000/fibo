import { expect, test } from 'bun:test';
import { execFileSync } from 'node:child_process';
import { readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

test('a quota copy the phone could not read still takes the computer’s next reading', () => {
  const output = join(tmpdir(), `mooring-quota-${crypto.randomUUID()}.json`);
  execFileSync(process.execPath, [new URL('./native/quota.ts', import.meta.url).pathname, output], {
    cwd: new URL('..', import.meta.url).pathname, encoding: 'utf8', timeout: 5000,
  });
  const result = JSON.parse(readFileSync(output, 'utf8'));
  rmSync(output);
  expect(result).toEqual({ hydrated: true, wrote: true, shown: 1, saved: 1 });
});
