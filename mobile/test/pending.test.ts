import { expect, test } from 'bun:test';
import { pendingFor, updatePending } from '../src/store/pending';

// Real phone 2026-11-18: 「立刻发送」 in a Claude conversation, then a deep link to a Codex one
// on the same chat screen; the Claude bubble still said 「已立刻发送」 under Codex.
test("a conversation's bubble on its way stays with it when another one opens", () => {
  updatePending('mac', 'claude-1', (p) => [...p, { text: '先看测试', at: 100, how: 'queue' }]);
  updatePending('mac', 'claude-1', (p) => p.map((x) => (x.how === 'queue' ? { ...x, now: true } : x)));

  expect(pendingFor('mac', 'codex-2')).toEqual([]);
  expect(pendingFor('other-mac', 'claude-1')).toEqual([]);

  // What the Codex screen does to its own list leaves Claude's alone.
  updatePending('mac', 'codex-2', (p) => [...p, { text: 'hi', at: 200 }]);
  updatePending('mac', 'codex-2', () => []);

  // Back on Claude: the bubble it should show is still there.
  expect(pendingFor('mac', 'claude-1')).toEqual([{ text: '先看测试', at: 100, how: 'queue', now: true }]);
  expect(pendingFor('mac', 'codex-2')).toEqual([]);
});

test('an unchanged list keeps its identity, so the screen does not redraw', () => {
  const before = pendingFor('mac', 'quiet');
  updatePending('mac', 'quiet', (p) => p.filter(() => true));
  expect(pendingFor('mac', 'quiet')).toBe(before);
});
