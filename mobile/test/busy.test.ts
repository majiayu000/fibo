import { expect, test } from 'bun:test';
import { whileBusy } from '../src/ui/busy';

// docs/phone-app/cli-turn-input.md: Enter while it works queues in Claude Code, goes in now in Codex.
test('a message sent while it works is described as the CLI treats it', () => {
  expect(whileBusy('claude').note).toContain('排队');
  expect(whileBusy('codex').note).toContain('插进去');
  expect(whileBusy('codex').note).not.toContain('排队');
  for (const other of ['opencode', null, undefined]) {
    const w = whileBusy(other);
    expect(w.note).not.toContain('排队');
    expect(w.note).not.toContain('插进去');
  }
});
