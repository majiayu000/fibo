// How much of the Agent's words the phone draws wrongly, measured on this computer's logs.
//   bun tools/md-corpus.tsx [days]        real replies from ~/.claude and ~/.codex
//   bun tools/md-corpus.tsx --synth       the benchmark's made-up replies, to check their shape
// Prints counts only, never a word of any log; nothing is written to disk.
//
// Each reply is rendered by the real <Markdown> (React Native stubbed to plain elements) and by
// the old splitter + regex, and both are compared with an independent reading of the same text.
import { plugin } from 'bun';
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { createElement, type ComponentType, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { replies } from '../bench/synth';
import { blocks } from './legacy-blocks';

const CHROME = 'md-chrome';
plugin({
  name: 'stub native',
  setup(b) {
    const el = (tag: string) => (p: { children?: ReactNode; testID?: string }) => (p.testID === CHROME ? null : createElement(tag, null, p.children));
    b.module('react-native', () => ({
      loader: 'object',
      exports: {
        Text: el('span'),
        View: el('div'),
        ScrollView: el('div'),
        Pressable: el('div'),
        Linking: { openURL: async () => undefined },
        Platform: { OS: 'ios', select: (o: { ios: unknown }) => o.ios },
        StyleSheet: { create: <T,>(x: T) => x, hairlineWidth: 1 },
        useWindowDimensions: () => ({ width: 390, height: 844 }),
        useColorScheme: () => 'light',
      },
    }));
    b.module('expo-clipboard', () => ({ loader: 'object', exports: { setStringAsync: async () => undefined } }));
  },
});

const { Markdown } = (await import('../src/ui/markdown')) as unknown as { Markdown: ComponentType<Record<string, unknown>> };
const { parseMarkdown, readable } = await import('../src/ui/md/parse');
const { marked } = await import('marked');

type Reply = { agent: string; final: boolean; text: string };

function* walk(dir: string): Generator<string> {
  let names: string[] = [];
  try {
    names = readdirSync(dir);
  } catch {
    return;
  }
  for (const n of names) {
    const p = join(dir, n);
    let st;
    try {
      st = statSync(p);
    } catch {
      continue;
    }
    if (st.isDirectory()) {
      if (n !== 'subagents') yield* walk(p);
    } else if (n.endsWith('.jsonl')) yield p;
  }
}

function fromLogs(days: number): Reply[] {
  const since = Date.now() - days * 86400_000;
  const out: Reply[] = [];
  const seen = new Set<string>();
  const add = (agent: string, text: string, final: boolean) => {
    text = text.replace(/^\n+|\n+$/g, '');
    if (!text.trim()) return;
    const h = createHash('sha1').update(text).digest('hex');
    if (seen.has(h)) return;
    seen.add(h);
    out.push({ agent, final, text });
  };
  const textOf = (blocksIn: unknown, kinds: string[]) =>
    Array.isArray(blocksIn) ? blocksIn.filter((b) => !kinds.length || kinds.includes(b?.type)).map((b) => b?.text ?? '').join('\n') : '';
  for (const [root, agent] of [[join(homedir(), '.claude/projects'), 'claude'], [join(homedir(), '.codex/sessions'), 'codex']] as const) {
    for (const path of walk(root)) {
      if (statSync(path).mtimeMs < since) continue;
      for (const line of readFileSync(path, 'utf8').split('\n')) {
        if (!line.includes('assistant')) continue;
        let d;
        try {
          d = JSON.parse(line);
        } catch {
          continue;
        }
        if (agent === 'claude') {
          if (d.type !== 'assistant' || d.isSidechain || d.isApiErrorMessage || d.message?.model === '<synthetic>') continue;
          add(agent, textOf(d.message?.content, ['text']), ['end_turn', 'stop_sequence'].includes(d.message?.stop_reason));
        } else {
          const p = d.payload;
          if (d.type !== 'response_item' || p?.type !== 'message' || p?.role !== 'assistant') continue;
          add(agent, textOf(p.content, []), p.phase === 'final_answer' || p.channel === 'final');
        }
      }
    }
  }
  return out;
}

// ---- what each renderer puts on screen ----
const INLINE = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*|__[^_\n]+__)|(\*[^*\s][^*\n]*\*|_[^_\s][^_\n]*_)|(\[[^\]\n]+\]\([^)\s]+\))/g;
function oldInline(text: string) {
  let out = '';
  let at = 0;
  let dead = 0;
  for (const m of text.matchAll(INLINE)) {
    out += text.slice(at, m.index);
    const [whole, code, bold, em, link] = m;
    if (code) out += code.slice(1, -1);
    else if (bold) out += bold.slice(2, -2);
    else if (em) out += em.slice(1, -1);
    else if (link) {
      const [, label, url] = link.match(/^\[([^\]]+)\]\(([^)]+)\)$/)!;
      out += label;
      if (!/^https?:/.test(url)) dead++;
    } else out += whole;
    at = m.index! + whole.length;
  }
  return { out: out + text.slice(at), dead };
}
function before(text: string) {
  let vis = '';
  let dead = 0;
  for (const b of blocks(text)) {
    if (b.kind === 'code') vis += b.text;
    else if (b.kind !== 'rule') {
      const r = oldInline(b.text);
      vis += r.out;
      dead += r.dead;
    }
  }
  return { vis, dead };
}
const THEME = { well: '', line: '', link: '', dim: '', ok: '' };
const ENTITY: Record<string, string> = { '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&#x27;': "'", '&#39;': "'" };
const decode = (s: string) => s.replace(/&(?:amp|lt|gt|quot|#x27|#39);/g, (m) => ENTITY[m]);
/** Each Text's words on its own line, so a raw table row can be told from a drawn cell. */
function after(text: string) {
  const html = renderToStaticMarkup(createElement(Markdown, { text, t: THEME, color: '', fold: false }));
  return decode(html.replace(/<\/?(?:div|span)>/g, '\u0001'));
}

// ---- the reference ----
type Tally = Record<string, number>;
function reference(tree: unknown) {
  let vis = '';
  const f: Tally = {};
  const hit = (k: string) => (f[k] = (f[k] ?? 0) + 1);
  const go = (n: { type: string; value?: string; alt?: string; url?: string; ordered?: boolean; checked?: boolean | null; lang?: string; children?: never[] }, up: string[]) => {
    if (n.type === 'text') vis += n.value;
    if (n.type === 'inlineCode') {
      vis += n.value;
      hit('inlineCode');
      if (up.some((u) => ['strong', 'emphasis', 'link'].includes(u))) hit('codeInEmphOrLink');
    }
    if (n.type === 'code') {
      vis += n.value;
      hit('code');
      if (up.includes('listItem')) hit('codeInList');
    }
    if (n.type === 'html') {
      vis += n.value;
      hit('html');
    }
    if (n.type === 'image' || n.type === 'imageReference') {
      vis += n.alt || (n.url ?? '').split(/[/\\]/).pop();
      hit('image');
    }
    if (n.type === 'link') {
      const v = (n.children?.[0] as { value?: string } | undefined)?.value;
      hit(/^https?:/.test(n.url ?? '') && v === n.url ? 'bareUrl' : 'link');
      if (!/^https?:/.test(n.url ?? '')) hit('localLink');
    }
    if (n.type === 'list' && up.includes('listItem')) hit('nestedList');
    if (n.type === 'listItem' && typeof n.checked === 'boolean') hit('task');
    for (const k of ['table', 'delete', 'heading', 'blockquote', 'list', 'strong', 'emphasis', 'thematicBreak', 'linkReference', 'footnoteReference']) if (n.type === k) hit(k);
    for (const c of n.children ?? []) go(c, [...up, n.type]);
  };
  go(tree as never, []);
  return { vis, f };
}
const norm = (s: string) => s.replace(/[\s\u0001]+/g, '');
const count = (s: string, c: string) => s.split(c).length - 1;

/** Lost or leftover markup: the same detectors for both renderers. */
function faults(vis: string, truth: string): Tally {
  const c: Tally = {};
  if (count(vis, '_') < count(truth, '_')) c['严重:下划线被吃'] = 1;
  if (count(vis, '*') < count(truth, '*')) c['严重:星号被吃'] = 1;
  if (count(vis, '`') > count(truth, '`')) c['中等:反引号外露'] = 1;
  if (count(vis, '~~') > count(truth, '~~')) c['中等:删除线外露'] = 1;
  if (count(vis, '*') > count(truth, '*')) c['中等:星号外露'] = 1;
  if (count(vis, '\\') > count(truth, '\\')) c['中等:转义外露'] = 1;
  if (count(vis, '&') > count(truth, '&')) c['中等:实体外露'] = 1;
  return c;
}

const days = process.argv[2] === '--synth' ? 0 : Number(process.argv[2] ?? 21);
const rows: Reply[] = days ? fromLogs(days) : replies(4000, 3).map((r) => ({ agent: 'synth', final: r.final, text: r.text }));

const scopes: Record<string, (r: Reply) => boolean> = {
  全部: () => true,
  最终: (r) => r.final,
  'Claude最终': (r) => r.agent === 'claude' && r.final,
  'Codex最终': (r) => r.agent === 'codex' && r.final,
};
const stats: Record<string, Tally> = Object.fromEntries(Object.keys(scopes).map((k) => [k, {}]));
const lengths: number[] = [];
let errors = 0;

for (const r of rows) {
  const c: Tally = { 条数: 1 };
  const src = readable(r.text);
  lengths.push(r.text.length);
  try {
    const { vis: truthVis, f } = reference(parseMarkdown(r.text));
    for (const k of Object.keys(f)) c[`有:${k}`] = 1;
    // Before
    const old = before(r.text);
    const oldTruth = reference(parseMarkdown(r.text.includes('<oai-mem-citation>') ? src : r.text)).vis;
    const b = faults(old.vis, oldTruth);
    const citation = r.text !== src;
    if (f.table) b['严重:表格原样'] = 1;
    if (f.image) b['中等:图片外露'] = 1;
    if (f.html || citation) b['中等:HTML外露'] = 1;
    if (f.task) b['中等:任务列表外露'] = 1;
    if (old.dead) b['轻微:本地链接点不动'] = 1;
    if (f.bareUrl) b['轻微:裸网址点不了'] = 1;
    if (f.codeInList) b['轻微:列表内代码贴左'] = 1;
    if (norm(old.vis) !== norm(oldTruth)) b['差异:可见文字'] = 1;
    // After, through the real component.
    const shown = after(r.text);
    const a = faults(shown, truthVis);
    if (shown.split('\u0001').some((t) => /^\s*\|.*\|\s*$/m.test(t) && /\|\s*:?-{2,}/.test(t))) a['严重:表格原样'] = 1;
    if (/!\[[^\]\n]*\]\(/.test(shown)) a['中等:图片外露'] = 1;
    if (shown.includes('oai-mem-citation')) a['中等:HTML外露'] = 1;
    if (norm(shown) !== norm(truthVis)) a['差异:可见文字'] = 1;
    // An independent parser as a second opinion on the words, markup characters aside.
    const strip = (s: string) => norm(s).replace(/[*_`~\\]/g, '');
    const other = decode(String(marked.parse(src, { async: false })).replace(/<img[^>]*alt="([^"]*)"[^>]*>/g, '$1').replace(/<[^>]+>/g, ''));
    const tagless = (s: string) => strip(s.replace(/<[^>\n]+>/g, ''));
    if (tagless(shown) !== tagless(other)) c['后:与marked文字不同'] = 1;
    const tier = (x: Tally) => ({ 严重: Object.keys(x).some((k) => k.startsWith('严重')), 中等: Object.keys(x).some((k) => k.startsWith('中等')), 轻微: Object.keys(x).some((k) => k.startsWith('轻微')) });
    for (const [prefix, x] of [['前', b], ['后', a]] as const) {
      for (const k of Object.keys(x)) c[`${prefix}:${k}`] = 1;
      const v = tier(x);
      if (v.严重) c[`${prefix}:档:严重`] = 1;
      if (v.严重 || v.中等) c[`${prefix}:档:严重或中等`] = 1;
      if (v.严重 || v.中等 || v.轻微) c[`${prefix}:档:任一`] = 1;
    }
  } catch {
    errors++;
    c['渲染抛错'] = 1;
  }
  for (const [k, pred] of Object.entries(scopes)) if (pred(r)) for (const [m, v] of Object.entries(c)) stats[k][m] = (stats[k][m] ?? 0) + v;
}

lengths.sort((a, b) => a - b);
const q = (p: number) => lengths[Math.floor(lengths.length * p)];
console.log(`条数 ${rows.length}，长度 p50/p90/p99/最长 ${q(0.5)}/${q(0.9)}/${q(0.99)}/${lengths.at(-1)}，渲染抛错 ${errors}`);
const names = Object.keys(scopes).filter((k) => stats[k].条数);
console.log(['指标', ...names].join('\t'));
for (const m of [...new Set(names.flatMap((n) => Object.keys(stats[n])))].sort()) {
  console.log([m, ...names.map((n) => {
    const v = stats[n][m] ?? 0;
    return m === '条数' ? String(v) : `${v} (${((v / stats[n].条数) * 100).toFixed(1)}%)`;
  })].join('\t'));
}
