import { expect, test } from 'bun:test';
import { HOW_NOTE, QUEUED_TAP, sendsNow, steerable, whileBusy } from '../src/ui/busy';

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

// The choice shows only for CLIs whose queue and steer keys the computer knows (host turn_input.rs).
test('queue or steer is offered only where both were measured, and says what was pressed', () => {
  expect(steerable('claude') && steerable('codex')).toBe(true);
  expect(steerable('opencode') || steerable(null)).toBe(false);
  expect(HOW_NOTE.queue).toContain('排队');
  expect(HOW_NOTE.steer).toContain('插进去');
});

// A queued message offers 「立刻发送」 only where its key was measured: Claude's ctrl+enter on an
// empty box sends the queue (2026-11-09); Codex and the rest were not measured.
test('send now is offered only on the CLI it was measured on', () => {
  expect(sendsNow('claude')).toBe(true);
  for (const other of ['codex', 'opencode', null, undefined]) expect(sendsNow(other)).toBe(false);
  expect(QUEUED_TAP).toContain('立刻发送');
});
