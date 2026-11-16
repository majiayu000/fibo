import { expect, test } from 'bun:test';
import { join } from 'node:path';

test('approval feedback uses the real actions with isolated native mocks', async () => {
  const run = Bun.spawn([process.execPath, 'test', join(import.meta.dir, 'native/approval.ts')], { stdout:'pipe', stderr:'pipe' });
  const [code, out, err] = await Promise.all([run.exited, new Response(run.stdout).text(), new Response(run.stderr).text()]);
  expect({code, output: code ? out+err : ''}).toEqual({code:0,output:''});
});
