// A tool's text as the phone holds and draws it: bytes against the machine's offsets, the gap
// between head and tail, rows of one height however long a line is.
import { describe, expect, test } from 'bun:test';
import { allText, byteLength, lastLines, bytes, columns, commandSteps, extend, loaded, metaWords, missing, MOST_COLUMNS, opensAtEnd, textRows, took, toolName, viewRows } from '../src/ui/toolview';

const utf8 = (s: string) => new TextEncoder().encode(s).length;

describe('a terminal in its sheet', () => {
  test('opens at the end only when it does not fit', () => {
    expect(opensAtEnd(12, 440, 19)).toBe(false);
    expect(opensAtEnd(40, 440, 19)).toBe(true);
  });
});

describe('words for a call', () => {
  test('sizes and times', () => {
    expect([bytes(512), bytes(1025), bytes(3 * 1024 * 1024)]).toEqual(['512 B', '2 KB', '3.0 MB']);
    expect([took(undefined), took(0), took(2500), took(125_000), took(120_000)]).toEqual(['', '不到 1 秒', '3 秒', '2 分 5 秒', '2 分钟']);
  });
  test('names read as people say them', () => {
    expect(toolName('mcp__playwright__browser_navigate')).toBe('playwright · browser_navigate');
    expect([toolName('exec'), toolName('exec_command'), toolName('Read')]).toEqual(['脚本', '命令', 'Read']);
  });
  test('a failure comes first, with its code when it has one', () => {
    const c = { id: 'a', name: 'Bash', kind: 'command', label: 'x', at: 1, took_ms: 3000, size: 2048 };
    expect(metaWords({ ...c, failed: true, code: 101 })).toBe('失败 · 退出码 101 · 3 秒 · 2 KB');
    expect(metaWords({ ...c, failed: true })).toBe('失败 · 3 秒 · 2 KB');
    expect(metaWords({ ...c, failed: false })).toBe('3 秒 · 2 KB');
    expect(metaWords({ ...c, failed: false, open: true })).toBe('还没有结果 · 3 秒');
    expect(metaWords({ ...c, failed: false }, false)).toBe('3 秒 · 2 KB');
  });
});

describe('a long text in pieces', () => {
  test('bytes counted as the machine counts them', () => {
    for (const s of ['abc', '中文', '🙂x', 'é', '"\\\n', '𠀋中🙂']) expect(byteLength(s)).toBe(utf8(s));
  });
  test('the head grows to the tail and nothing is lost', () => {
    const whole = 'head\n中段一\n中段二\ntail\n';
    const head = 'head\n';
    const tail = 'tail\n';
    const tailAt = utf8(whole) - utf8(tail);
    let l = loaded({ total: utf8(whole), head, tail, tail_at: tailAt });
    let end = byteLength(head);
    expect(missing(l, end)).toBe(tailAt - end);
    const rows = viewRows(l, end, 'text');
    expect(rows.map((r) => r.kind)).toEqual(['line', 'gap', 'line']);
    for (const piece of ['中段一\n', '中段二\n']) {
      l = extend(l, piece);
      end += utf8(piece);
    }
    expect(missing(l, end)).toBe(0);
    expect(allText(l)).toBe(whole);
    expect(viewRows(l, end, 'text').every((r) => r.kind === 'line')).toBe(true);
  });
  test('a whole short text has no gap and no extra empty row', () => {
    const l = loaded({ total: 4, head: 'a\nb\n', tail: '', tail_at: 4 });
    expect(viewRows(l, 4, 'text')).toEqual([
      { kind: 'line', text: 'a', tone: undefined },
      { kind: 'line', text: 'b', tone: undefined },
    ]);
  });
});

describe('rows', () => {
  test('a diff colours its lines, other text does not', () => {
    const l = loaded({ total: 8, head: '- a\n+ b\nc\n', tail: '', tail_at: 10 });
    const tones = (kind: string) => viewRows(l, 10, kind).map((r) => (r.kind === 'line' ? r.tone : null));
    expect(tones('diff')).toEqual(['del', 'add', undefined]);
    expect(tones('text')).toEqual([undefined, undefined, undefined]);
  });
  test('a very long line goes on over many rows, none cut and none too wide', () => {
    const line = 'x'.repeat(MOST_COLUMNS * 2 + 5);
    const rows = viewRows(loaded({ total: line.length, head: line, tail: '', tail_at: line.length }), line.length, 'text');
    expect(rows.length).toBe(3);
    expect(rows.map((r) => (r.kind === 'line' ? r.text : '')).join('')).toBe(line);
    expect(columns(rows)).toBe(MOST_COLUMNS);
  });
  test('CJK takes two columns, tabs four', () => {
    expect(columns([{ kind: 'line', text: '中文a' }])).toBe(5);
    const rows = viewRows(loaded({ total: 2, head: '\tx', tail: '', tail_at: 2 }), 2, 'text');
    expect(rows[0]).toEqual({ kind: 'line', text: '    x', tone: undefined });
  });
  test('two hundred thousand lines stay cheap to lay out', () => {
    const big = 'a line of ordinary output\n'.repeat(200_000);
    const started = performance.now();
    const rows = viewRows(loaded({ total: big.length, head: big, tail: '', tail_at: big.length }), big.length, 'text');
    expect(rows.length).toBe(200_000);
    expect(performance.now() - started).toBeLessThan(500);
  });
});

describe('commandSteps', () => {
  test('a chained command reads a step a line', () => {
    expect(commandSteps('cd /a && FOO=1 cargo test 2>&1 | tail -3; echo done')).toBe('cd /a &&\nFOO=1 cargo test 2>&1\n| tail -3;\necho done');
  });
  test('quotes, a here-document and a redirect stay whole', () => {
    expect(commandSteps(`git commit -m 'a && b; c | d'`)).toBe(`git commit -m 'a && b; c | d'`);
    expect(commandSteps('cat <<E\na && b\nE')).toBe('cat <<E\na && b\nE');
    expect(commandSteps('echo x >| f')).toBe('echo x >| f');
  });
});

test('terminal text is one row a line, however long the line', () => {
  const long = 'Bash(cd /tmp; R=x; for f in a b c; do cat "$f"; done)'.repeat(4);
  expect(textRows(`${long}\n\n中文\n`)).toEqual([{ kind: 'line', text: long, tone: undefined }, { kind: 'line', text: '', tone: undefined }, { kind: 'line', text: '中文', tone: undefined }]);
});

describe('the live block', () => {
  test('shows a screen by its last lines, not the blank rows under them', () => {
    expect(lastLines('a\nb  \nc\n\n   \n', 2)).toEqual(['b', 'c']);
    expect(lastLines('only', 8)).toEqual(['only']);
  });
  test('leaves out the input box and the footer under it', () => {
    const rule = '─'.repeat(40);
    const claude = ['⏺ Bash(ping -c 90 127.0.0.1)', '  ⎿  64 bytes from 127.0.0.1', '', '✶ Thundering… (36s)', '', rule, '❯ Press up to edit queued messages', rule, '  🔥7天', '  ⏵⏵ bypass permissions on'].join('\n');
    expect(lastLines(claude, 8)).toEqual(['⏺ Bash(ping -c 90 127.0.0.1)', '  ⎿  64 bytes from 127.0.0.1', '', '✶ Thundering… (36s)']);
    // A rule far above the bottom is the Agent's own output, not its box.
    const table = [rule, 'a', rule, '1', '2', '3', '4', '5', '6', '7', '8'].join('\n');
    expect(lastLines(table, 3)).toEqual(['6', '7', '8']);
  });
});
