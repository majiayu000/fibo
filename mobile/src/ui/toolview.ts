// A tool call's words and text as the two tool screens draw them, apart from React Native
// so `bun test` runs them. A long text comes as its two ends with a gap to fetch between
// them; what is drawn is fixed-height rows, however long the text or its lines.
import { tr } from '../i18n';

export type ToolCall = {
  id: string;
  name: string;
  kind: string;
  label: string;
  /** What the Agent said it does. */
  about?: string;
  at: number;
  took_ms?: number;
  failed: boolean;
  code?: number;
  open?: boolean;
  size?: number;
};
export type ToolText = { total: number; head: string; tail: string; tail_at: number };
export type ToolDetail = ToolCall & {
  input_kind: string;
  input: ToolText;
  output?: ToolText;
  steps?: { tool: string; label: string; unread?: boolean }[];
  note?: string;
};

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.ceil(n / 1024)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function took(ms: number | undefined): string {
  if (ms === undefined) return '';
  if (ms < 1000) return tr('不到 1 秒');
  const s = Math.round(ms / 1000);
  if (s < 60) return tr('{s} 秒', { s });
  const m = Math.floor(s / 60);
  return s % 60 ? tr('{m} 分 {s} 秒', { m, s: s % 60 }) : tr('{m} 分钟', { m });
}

/** The tool's own name, an MCP tool's as `server · tool`. */
export function toolName(name: string): string {
  const short = name.startsWith('mcp__') ? name.slice(5).replace('__', ' · ') : name;
  return short === 'exec' ? tr('脚本') : short === 'exec_command' || short === 'Bash' ? tr('命令') : short;
}

/** The dim line under a call: what happened to it, how long, how much. A title that already
 * names the tool leaves it out. */
export function metaWords(call: ToolCall, named = true): string {
  const ending = call.failed ? (call.code !== undefined && call.code !== 0 ? tr('失败 · 退出码 {code}', { code: call.code }) : tr('失败')) : call.open ? tr('还没有结果') : '';
  return [ending, named && call.kind !== 'command' ? toolName(call.name) : '', took(call.took_ms), call.size !== undefined && !call.open ? bytes(call.size) : ''].filter(Boolean).join(' · ');
}

/** A text as held on the phone while its page is open: the head grows towards the tail. */
export type Loaded = { total: number; head: string; tail: string; tailAt: number };

export const loaded = (t: ToolText): Loaded => ({ total: t.total, head: t.head, tail: t.tail, tailAt: t.tail_at });

/** UTF-8 bytes: the machine's offsets count bytes, the phone's strings do not. */
export function byteLength(text: string): number {
  let n = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c < 0xdc00) {
      n += 4;
      i++;
    } else n += 3;
  }
  return n;
}

/** Where the next stretch begins, and how much is still to fetch. */
export function headEnd(l: Loaded, known?: number): number {
  return known ?? byteLength(l.head);
}
export const missing = (l: Loaded, end: number) => Math.max(0, l.tailAt - end);

/** A fetched stretch on the end of the head, up to where the tail begins. */
export function extend(l: Loaded, text: string): Loaded {
  return { ...l, head: l.head + text };
}

/** All of it, when nothing is missing; else what has come, in order. */
export const allText = (l: Loaded) => l.head + l.tail;

export type ViewRow = { kind: 'line'; text: string; tone?: 'add' | 'del' } | { kind: 'gap' };

/** Longest row drawn: a line past this goes on over the next rows, none is cut. */
export const MOST_COLUMNS = 1000;

/** A shell command read as steps: a new line after `&&`, `||` and `;`, before `|`, outside
 * quotes. Only for reading; a copy keeps the text as it ran. A here-document stays as it is. */
export function commandSteps(text: string): string {
  if (text.includes('<<')) return text;
  let out = '';
  let quote = '';
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '\\') {
      out += c + (text[++i] ?? '');
      continue;
    }
    if (quote) {
      if (c === quote) quote = '';
      out += c;
      continue;
    }
    if (c === '"' || c === "'") quote = c;
    const two = text.slice(i, i + 2);
    if (two === '&&' || two === '||') {
      out += `${two}\n`;
      i++;
      while (text[i + 1] === ' ') i++;
    } else if (c === ';' && text[i + 1] !== ';' && text[i - 1] !== ';') {
      out += ';\n';
      while (text[i + 1] === ' ') i++;
    } else if (c === '|' && text[i - 1] !== '>') {
      out = `${out.trimEnd()}\n| `;
      while (text[i + 1] === ' ') i++;
    } else out += c;
  }
  return out;
}

function pushLines(rows: ViewRow[], text: string, diff: boolean) {
  if (!text) return;
  const lines = text.split('\n');
  // A text ending in a newline has no empty last line to draw.
  if (lines.at(-1) === '') lines.pop();
  for (const raw of lines) {
    const line = raw.replaceAll('\t', '    ');
    const tone = !diff ? undefined : line.startsWith('+') ? ('add' as const) : line.startsWith('-') ? ('del' as const) : undefined;
    if (line.length <= MOST_COLUMNS) {
      rows.push({ kind: 'line', text: line, tone });
      continue;
    }
    for (let at = 0; at < line.length; at += MOST_COLUMNS) rows.push({ kind: 'line', text: line.slice(at, at + MOST_COLUMNS), tone });
  }
}

/** The rows of a text: head, the gap while one remains, tail. */
export function viewRows(l: Loaded, end: number, kind: string): ViewRow[] {
  const rows: ViewRow[] = [];
  const diff = kind === 'diff';
  const shown = (t: string) => (kind === 'command' ? commandSteps(t) : t);
  pushLines(rows, shown(l.head), diff);
  if (missing(l, end) > 0) rows.push({ kind: 'gap' });
  pushLines(rows, shown(l.tail), diff);
  return rows;
}

/**
 * A terminal's marks the phone's monospace face lacks, as ones it has: Android drew Claude's
 * `⏵⏵` as two boxes and `⏺` as an orange emoji (real phone, 2026-11-09 and 11-12).
 */
const GLYPHS: Record<string, string> = { '⏵': '▸', '⏴': '◂', '⏸': '‖', '⏺': '●' };
export const screenText = (text: string) => text.replace(/[⏵⏴⏸⏺]/g, (c) => GLYPHS[c]);

/** A whole text as rows, none wrapped. */
export const textRows = (text: string): ViewRow[] => {
  const rows: ViewRow[] = [];
  pushLines(rows, text, false);
  return rows;
};

/** How wide the longest row is drawn, in a monospace face where CJK takes two columns. */
export function columns(rows: ViewRow[]): number {
  let most = 0;
  for (const r of rows) {
    if (r.kind !== 'line') continue;
    let n = 0;
    for (let i = 0; i < r.text.length; i++) n += r.text.charCodeAt(i) > 0x2e7f ? 2 : 1;
    if (n > most) most = n;
  }
  return most;
}

/** Whether a list of `rows` this tall opens scrolled to its end: a text that fits has no end
 * to go to, and a list told to start on its last row draws only that one. */
export const opensAtEnd = (rows: number, height: number, line: number) => rows * line > height;

/** A rule across the screen: the edges of a CLI's input box. */
const RULE = /^\s*[─━]{8,}/;

/**
 * What a screen shows the Agent doing, as its last `n` lines: the input box and everything
 * under it cut off (the box between its last two rules, then the footer), blank lines at the
 * bottom gone. Its last lines alone were Claude's box, tips and footer (2026-11-09).
 */
export function lastLines(text: string, n: number): string[] {
  let lines = screenText(text).replace(/\s+$/, '').split('\n').map((l) => l.replace(/\s+$/, ''));
  const rules = lines.map((l, i) => (RULE.test(l) ? i : -1)).filter((i) => i >= 0);
  if (rules.length >= 2 && lines.length - rules[rules.length - 1] <= 6) lines = lines.slice(0, rules[rules.length - 2]);
  while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
  return lines.slice(-n);
}
