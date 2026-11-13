import { expect, test } from 'bun:test';
import { openable, parseMarkdown, parsed, pathOf, readable } from '../src/ui/md/parse';
import { columnWidths, monoWidth, textWidth } from '../src/ui/md/table';

type N = { type: string; value?: string; url?: string; lang?: string; children?: N[] };
const kinds = (text: string) => (parseMarkdown(text).children as N[]).map((n) => n.type);
const first = (text: string) => (parseMarkdown(text).children as N[])[0];
const find = (n: N, type: string): N | undefined => (n.type === type ? n : n.children?.map((c) => find(c, type)).find(Boolean));

test('a reply splits into the blocks it is drawn from', () => {
  const md = ['## 结论', '还差两件事：', '', '1. 手机端轮询', '   - 每 2 秒', '2. 断线补齐', '', '```rust', 'fn main() {}', '```', '> 引用', '', '---', '', '| a | b |', '|---|---|', '| 1 | 2 |'].join('\n');
  expect(kinds(md)).toEqual(['heading', 'paragraph', 'list', 'code', 'blockquote', 'thematicBreak', 'table']);
  expect(find(parseMarkdown(md) as N, 'code')?.lang).toBe('rust');
});

test('an unclosed fence keeps the rest as code', () => {
  expect(kinds('看这个\n\n```\na\nb')).toEqual(['paragraph', 'code']);
});

test('bold closes after Chinese punctuation', () => {
  const p = first('**结论：**后面');
  expect(p.children?.map((c) => c.type)).toEqual(['strong', 'text']);
  expect(p.children?.[1].value).toBe('后面');
});

test('underscores inside words stay', () => {
  const p = first('改 foo_bar_baz 和 __init__.py 两处');
  expect(p.children?.some((c) => c.type === 'emphasis')).toBe(false);
  expect(p.children?.[0].value).toContain('foo_bar_baz');
});

test('code inside bold and inside a link is code', () => {
  expect(find(first('**`a.rs`** 改了'), 'inlineCode')?.value).toBe('a.rs');
  expect(find(first('见 [`a.rs`](crates/a.rs:12)'), 'inlineCode')?.value).toBe('a.rs');
});

test('a table right under a sentence is still a table', () => {
  expect(kinds('如下：\n| a | b |\n|---|---|\n| 1 | 2 |')).toEqual(['paragraph', 'table']);
});

test('bare addresses become links', () => {
  expect(find(first('看 https://x.dev/a_b_c 这里'), 'link')?.url).toBe('https://x.dev/a_b_c');
});

test('Codex citations are neither drawn nor copied', () => {
  const text = '做完了。\n\n<oai-mem-citation>\n<citation_entries>\nA.md:1|note=[x]\n</citation_entries>\n</oai-mem-citation>';
  expect(readable(text)).toBe('做完了。');
  expect(kinds(text)).toEqual(['paragraph']);
  expect(readable('`<oai-mem-citation>` 是个标签')).toBe('`<oai-mem-citation>` 是个标签');
});

test('text with no markup skips the parser and keeps its paragraphs', () => {
  expect(kinds('第一段\n\n第二段')).toEqual(['paragraph', 'paragraph']);
  expect(parsed('同一句')).toBe(parsed('同一句'));
});

test('only what a phone can open opens', () => {
  expect(openable('https://x.dev')).toBe(true);
  expect(openable('mailto:a@b.c')).toBe(true);
  expect(openable('crates/a.rs:12')).toBe(false);
  expect(openable('file:///Users/me/a.rs')).toBe(false);
  expect(pathOf('file:///Users/me/a%20b.rs')).toBe('/Users/me/a b.rs');
});

test('a table that fits fills the width; one that does not keeps its columns readable', () => {
  const fits = columnWidths([['项目', '结果'], ['a', 'b']], 13, 340);
  expect(fits.reduce((a, b) => a + b)).toBeLessThanOrEqual(340);
  expect(fits.reduce((a, b) => a + b)).toBeGreaterThan(330);
  const wide = columnWidths([['第一列的说明文字很长很长很长很长很长很长很长很长很长很长', 'crates/fibo-core/src/content/plain.rs', '第三列的说明文字也很长很长很长很长很长很长很长很长']], 13, 340);
  expect(wide.every((w) => w >= 56 && w <= 220)).toBe(true);
  expect(wide.reduce((a, b) => a + b)).toBeGreaterThan(340);
  // No column narrower than its text a few lines tall: no towers of single characters.
  expect(wide[2]).toBeGreaterThanOrEqual(90);
});

test('widths count CJK as a square', () => {
  expect(textWidth('中文', 10)).toBe(20);
  expect(monoWidth('ab\n中', 10)).toBe(12);
});
