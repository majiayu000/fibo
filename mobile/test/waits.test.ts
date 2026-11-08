// Every way an Agent can wait on you gets a way forward on the phone. The waits are the
// host's own list (crates/mooring-core/assets/reasons.json): one added there fails here until
// it has a sample below and the phone's answer to it is decided (2026-11-08: a question the
// phone could not read left 「等你回答」 and Esc, and the conversation stuck).
import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { sendBlocked, waitView } from '../src/ui/waits';

type Rule = { id: string; kind: string; match: string };
const rules: Rule[] = JSON.parse(readFileSync(new URL('../../crates/mooring-core/assets/reasons.json', import.meta.url), 'utf8')).rules ?? [];

/** What the host puts in `waiting` for each, as written by the code or CLI that says it. */
const SAMPLES: Record<string, string> = {
  subagent: '子 Agent worker：等待你的输入',
  'question-header': '【下一步】接下来怎么继续？\n1. 新对话继续\n2. 就在这里继续',
  'claude-use-tool': 'Claude requested permissions to use mcp__github__create_issue',
  'claude-read-path': 'Claude requested permissions to read from /etc/hosts',
  'claude-write-path': 'Claude requested permissions to write to /tmp/a.txt',
  'bash-part': 'This command requires approval: rm -rf build',
  'command-approval': 'Waiting for approval',
  'screen-edit': 'Do you want to make this edit to main.rs?',
  'screen-create': 'Do you want to create notes.md?',
  'screen-proceed': 'Would you like to proceed?',
  'hook-permission': 'CLI 请求工具审批（Hook）',
  'hook-mcp-form': 'MCP 表单等待填写（Hook）',
  'hook-mcp-browser': 'MCP 请求在浏览器中完成（Hook）',
  'official-sandbox': '等待沙箱授权',
  'official-worker': '后台任务在等你回应',
  'official-dialog': '有对话框打开',
  'generic-input': 'Agent 等待你的输入（Hook）',
};

/** reasons.json is read by Rust: `(?P<name>` and a leading `(?i)` in JavaScript's words. */
function regex(r: Rule): RegExp {
  const insensitive = r.match.startsWith('(?i)');
  return new RegExp(r.match.replace(/^\(\?i\)/, '').replace(/\(\?P</g, '(?<'), insensitive ? 'i' : '');
}

const waits = rules.filter((r) => r.kind === 'waiting');

describe('every wait has a way forward on the phone', () => {
  test('the host lists waits, and each one has a sample here', () => {
    expect(waits.length).toBeGreaterThan(10);
    expect(waits.map((r) => r.id).filter((id) => !(id in SAMPLES))).toEqual([]);
  });
  for (const r of waits) {
    test(r.id, () => {
      const sample = SAMPLES[r.id];
      expect(regex(r).test(sample)).toBe(true);
      const v = waitView(sample)!;
      expect(v.text.length).toBeGreaterThan(0);
      // Buttons, or the terminal and its keys, or a plain word that it takes the computer.
      expect(v.options.length > 0 || v.keys || v.computer.length > 0).toBe(true);
    });
  }
  test('only what a phone cannot do says to go to the computer', () => {
    expect(waitView(SAMPLES['hook-mcp-browser'])!.computer).toContain('浏览器');
    expect(waitView(SAMPLES['hook-mcp-form'])!.computer).toContain('电脑');
    expect(waitView(SAMPLES['hook-permission'])!.computer).toBe('');
  });
});

describe('typing while a menu is open', () => {
  const held = (id: string, again = false) => sendBlocked(waitView(SAMPLES[id]), SAMPLES[id], again);
  test('with options on screen it waits until one is picked, however often it is sent', () => {
    expect(held('question-header')).toContain('Esc');
    expect(held('question-header', true)).toContain('Esc');
  });
  test('a menu the phone cannot read warns once, and the same text sent again goes', () => {
    expect(held('hook-permission')).toContain('再点一次');
    expect(held('hook-permission', true)).toBe('');
  });
  test('a wait for words takes them at once', () => {
    expect(held('generic-input')).toBe('');
    expect(held('subagent')).toBe('');
    expect(sendBlocked(null, null, false)).toBe('');
  });
});
