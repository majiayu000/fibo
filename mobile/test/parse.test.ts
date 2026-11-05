// Reading what the machine sends: what an approval asks about, a question's
// options, a patch's lines, quota words, the relay's place, breadcrumbs, dangerous commands.
import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { Close, closeOutcome } from '../src/remote/close';
import { askFrom, asksTwice, partialWords, readBackWords, trustWords, checkLine, choices, compact, dangerous, diffLines, level, modelName, questionText, quotaView, relayPlace, resetText, runningFor, span, trail, windowName } from '../src/ui/parse';

/** The screens the machine's parser is held to, and what it reads off each. */
const fixture = (cli: string): Record<string, { ask?: { subject: string; kind: string } }> =>
  JSON.parse(readFileSync(new URL(`../../crates/mooring-host/tests/fixtures/screens/${cli}/expected.json`, import.meta.url), 'utf8'));

describe('an approval the machine read', () => {
  test('the real Tip screen asks about the command, and git push is dangerous', () => {
    const screens = fixture('claude');
    expect(screens['approval-bash-tip.txt'].ask?.subject).toBe('touch second.txt');
    const push = screens['approval-push-requires.txt'].ask!;
    expect(push.subject).toBe('git push origin main');
    expect(asksTwice(askFrom(push, 'Do you want to proceed?'))).toBe(true);
    expect(asksTwice(askFrom({ subject: 'touch second.txt', kind: 'Bash command' }, 'q'))).toBe(false);
    // Any line of a long command counts.
    expect(asksTwice(askFrom(screens['approval-multiline-real.txt'].ask!, 'q'))).toBe(true);
  });
  test('the "don\'t ask again" button says what its menu says, and is not there without one', () => {
    const screens = fixture('claude') as Record<string, { ask?: { subject: string; kind: string }; always_label?: string }>;
    const words = screens['approval-bash-tip.txt'].always_label!;
    expect(words).toContain('/Users/x/t');
    expect(askFrom({ subject: 'x', always: words }, 'q').always).toBe(words);
    expect(askFrom({ subject: 'x' }, 'q').always).toBe('');
    expect(askFrom(null, 'q').always).toBe('');
  });
  test('a tip line would not have asked twice: that was the bug', () => {
    expect(dangerous('Tip: auto mode handles these prompts for you')).toBe(false);
  });
  test('nothing read yet, or only part of it, asks twice whatever the words', () => {
    const none = askFrom(null, 'Do you want to proceed?');
    expect(none.subject).toBe('Do you want to proceed?');
    expect(none.known).toBe(false);
    expect(asksTwice(none)).toBe(true);
    expect(asksTwice(askFrom({ subject: '', kind: 'Bash command' }, 'q'))).toBe(true);
    expect(asksTwice(askFrom({ subject: 'ls', truncated: true }, 'q'))).toBe(true);
  });
});

describe('a folder asking to be trusted', () => {
  test('says what trusting allows, naming the Agent', () => {
    const w = trustWords('Codex');
    expect(w.ask).toContain('Codex');
    for (const word of ['读', '改', '运行', '文件夹']) expect(w.means).toContain(word);
  });
});

describe('a conversation read only from the end of its log', () => {
  test('says so, with messages and without', () => {
    expect(partialWords(3)).toContain('更早的消息没有读进来');
    expect(partialWords(0)).toContain('里面没有消息');
    expect(partialWords(0)).not.toBe(partialWords(3));
    expect(readBackWords(false, '')).toContain('点这里读');
    expect(readBackWords(true, '')).toContain('正在读');
    expect(readBackWords(false, '读不回')).toContain('再试一次');
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

describe('the gaps against the design', () => {
  test('a span reads as people say it', () => {
    expect(span(45)).toBe('45 秒');
    expect(span(720)).toBe('12 分钟');
    expect(span(3600)).toBe('1 小时');
    expect(span(4800)).toBe('1 小时 20 分钟');
  });
  test('a running turn says its time after its first minute', () => {
    expect(runningFor(1000, 1030)).toBe('');
    expect(runningFor(1000, 1720)).toBe('12 分钟');
    expect(runningFor(null, 1000)).toBe('');
  });
  test('tokens read short', () => {
    expect(compact(950)).toBe('950');
    expect(compact(12_800_000)).toBe('12.8M');
    expect(compact(3_282_342_605)).toBe('3.3B');
    expect(compact(125_500)).toBe('126K');
  });
  test('a model reads as people say it, an unknown one as it is', () => {
    expect(modelName('claude-opus-5-5')).toBe('Opus 5.5');
    expect(modelName('claude-sonnet-4-5-20250929')).toBe('Sonnet 4.5');
    expect(modelName('claude-opus-4-20250514')).toBe('Opus 4');
    expect(modelName('gpt-6.1-sol')).toBe('gpt-6.1-sol');
    expect(modelName('k3')).toBe('k3');
  });
  test('checks say what still covers the files', () => {
    const c = (command: string, ok: boolean, stale: boolean) => ({ command, ok, stale, at: 1 });
    expect(checkLine([])).toBeNull();
    expect(checkLine([c('tsc', true, false), c('cargo test', true, false)])).toEqual({ text: 'tsc 通过 · cargo test 通过', failed: false, stale: false });
    expect(checkLine([c('tsc', false, false), c('cargo test', true, true)])).toEqual({ text: 'tsc 没通过', failed: true, stale: false });
    expect(checkLine([c('cargo test', true, true)])).toEqual({ text: 'cargo test 通过，之后又改过', failed: false, stale: true });
  });
});
