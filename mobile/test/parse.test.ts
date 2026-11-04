// Reading what the machine sends: the command an approval asks about, a question's
// options, a patch's lines, quota words, the relay's place, breadcrumbs, dangerous commands.
import { describe, expect, test } from 'bun:test';
import { Close, closeOutcome } from '../src/remote/close';
import { choices, dangerous, diffLines, level, questionText, quotaView, readAsk, relayPlace, resetText, trail, windowName } from '../src/ui/parse';

const BOX = `╭──────────╮
│ Bash command                    │
│                                 │
│   git push origin main          │
│   Push 6 commits                │
│                                 │
│ Do you want to proceed?         │
│ ❯ 1. Yes                        │
│   2. Yes, and don't ask again for git push commands │
│   3. No (esc)                   │
╰──────────╯`;

describe('an approval on screen', () => {
  test('finds the command in a boxed prompt', () => {
    expect(readAsk(BOX, 'q')).toEqual({ subject: 'git push origin main', kind: 'Bash command' });
  });
  test('a prompt with no box and no such option', () => {
    const a = readAsk('Bash command\n  git push origin main\nDo you want to proceed?\n> 1. Yes\n  2. No', 'q');
    expect(a.subject).toBe('git push origin main');
  });
  test('falls back to the question when the screen says nothing yet', () => {
    expect(readAsk('', '要继续吗').subject).toBe('要继续吗');
  });
});

describe('dangerous commands', () => {
  test.each(['git push origin main', 'rm -rf build', 'cd x && rm y', 'fly deploy', 'npm publish', 'sudo reboot', 'DROP TABLE users'])('%s', (c) => {
    expect(dangerous(c)).toBe(true);
  });
  test.each(['ls -la', 'git status', 'cargo test', 'cat firmware.txt'])('%s is not', (c) => {
    expect(dangerous(c)).toBe(false);
  });
});

describe('a question', () => {
  const q = '用竖条纹还是保持平滑？\n❯ 1. 竖条纹\n  2. 保持平滑';
  test('offers its numbered options', () => {
    expect(choices(q)).toEqual([{ key: '1', label: '竖条纹' }, { key: '2', label: '保持平滑' }]);
    expect(questionText(q)).toBe('用竖条纹还是保持平滑？');
  });
  test('one numbered line is not a menu', () => {
    expect(choices('还差一件：\n1. 轮询')).toEqual([]);
    expect(choices(null)).toEqual([]);
  });
});

test('a patch numbers deletions by the old file, the rest by the new', () => {
  const lines = diffLines('diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -10,3 +10,3 @@ fn\n keep\n-old\n+new\n tail\n\\ No newline at end of file\n');
  expect(lines).toEqual([
    { kind: 'hunk', text: '@@ -10,3 +10,3 @@ fn' },
    { kind: 'ctx', no: 10, text: 'keep' },
    { kind: 'del', no: 11, text: 'old' },
    { kind: 'add', no: 11, text: 'new' },
    { kind: 'ctx', no: 12, text: 'tail' },
  ]);
});

describe('quota words', () => {
  test('windows have their usual names', () => {
    expect([300, 10080, 1440, 120].map(windowName)).toEqual(['5 小时窗口', '本周', '今天', '2 小时']);
  });
  test('a reset never reads as 60 minutes', () => {
    expect(resetText(1000 + 23 * 3600 + 59 * 60 + 50, 1000)).toBe('24 时后重置');
    expect(resetText(1000 + 100 * 60, 1000)).toBe('1 时 40 分后重置');
    expect(resetText(0, 1000)).toBe('');
  });
  test('orange starts past 80%', () => {
    expect([49, 50, 80, 81].map(level)).toEqual(['low', 'mid', 'mid', 'high']);
  });
});

test('a private relay address reads as the local network', () => {
  expect(relayPlace('ws://192.168.1.20:8790')).toEqual({ host: '192.168.1.20:8790', lan: true });
  expect(relayPlace('wss://relay.example.com/x')).toEqual({ host: 'relay.example.com', lan: false });
  expect(relayPlace('ws://172.32.0.1').lan).toBe(false);
});

test('breadcrumbs carry each step\'s full path', () => {
  expect(trail('/Users/d/code/AI', '/Users/d')).toEqual([
    { label: '~', path: '/Users/d' },
    { label: 'code', path: '/Users/d/code' },
    { label: 'AI', path: '/Users/d/code/AI' },
  ]);
  expect(trail('/opt/x', '/Users/d').map((c) => c.path)).toEqual(['/', '/opt', '/opt/x']);
});

test('a relay too old says so whatever its close code', () => {
  const o = closeOutcome(Close.MACHINE_GONE, '中继版本太旧（协议 2，这台 Mooring 要 3 以上）：请升级中继');
  expect(o.text).toContain('中继版本太旧');
  expect(closeOutcome(Close.MACHINE_GONE).text).toContain('不在线');
});

describe('quotaView', () => {
  test('an offline computer shows what it last said, or says it never did', () => {
    expect(quotaView(false, false, [{}])).toBe('gone');
    expect(quotaView(false, false, [])).toBe('gone');
    expect(quotaView(false, false, null)).toBe('never');
  });
  test('an online computer that cannot answer says so even with an old reading', () => {
    expect(quotaView(true, false, [{}])).toBe('unsupported');
  });
  test('an online computer shows readings, or waits for the first', () => {
    expect(quotaView(true, true, [{}])).toBe('live');
    expect(quotaView(true, true, null)).toBe('loading');
  });
});
