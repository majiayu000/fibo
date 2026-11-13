// Made-up replies with the measured shape of real ones (docs/phone-app/markdown.md gives the
// rates), so benchmarks run on something realistic without any log text in the app or repo.
// Every word here was written by hand; the corpus tool only ever contributed the numbers.
const PHRASES = [
  '这一步先把解析放到列表外面',
  '改完之后再跑一遍测试',
  '原因是缓存没有按会话分开',
  '键盘弹出时窗口不再缩小',
  '这里读的是电脑端的日志',
  '重连后补上中间缺的消息',
  '两个平台都走同一条路径',
  '错误信息现在写到标题栏下面',
  '旧数据不用迁移，从空库开始',
  '这个分支只改手机端',
  'the host writes one record per message',
  'retry only after the socket reports closed',
  '额度按 Agent 分开显示',
  '审批条只在等你时出现',
  '第二屏的高度算错了一行',
  '看板列的顺序保持不变',
];
const FILES = ['src/ui/kit.tsx', 'crates/app/src/layout.rs', 'mobile/app.json', 'src/remote/hub.ts', 'foo_bar_baz.rs', 'docs/plan.md', 'tests/managed_session.rs'];
const CODE_LINES = [
  'const page = await remote.request(\'chat\', { session, after: last.current, limit: 50 });',
  'fn plain_line(text: &str) -> String { text.lines().map(str::trim).collect() }',
  '# 先确认没有别的进程占着这个端口，再启动中继',
  'cargo test -p fibo-core --test guard',
  'if (!online) return;',
  'let merged = [...have, ...fresh].sort((a, b) => a.seq - b.seq);',
  'adb shell dumpsys gfxinfo dev.mooring.mobile reset',
  '}',
];

/** A small seeded generator: the same call gives the same replies on every run and device. */
function rng(seed: number) {
  let x = seed >>> 0 || 1;
  return () => {
    x ^= x << 13;
    x >>>= 0;
    x ^= x >>> 17;
    x ^= x << 5;
    x >>>= 0;
    return x / 0xffffffff;
  };
}

type R = () => number;
const pick = <T,>(r: R, a: T[]) => a[Math.floor(r() * a.length)];

/** Reply lengths by quantile, in characters, from 20,776 real replies. */
const MID_LENGTHS: [number, number][] = [[0, 12], [0.1, 29], [0.25, 42], [0.5, 61], [0.75, 89], [0.9, 142], [0.99, 271], [1, 800]];
const FINAL_LENGTHS: [number, number][] = [[0, 40], [0.1, 84], [0.25, 267], [0.5, 702], [0.75, 1107], [0.9, 1544], [0.99, 2874], [1, 4000]];

function lengthAt(table: [number, number][], q: number) {
  for (let i = 1; i < table.length; i++) {
    const [q0, l0] = table[i - 1];
    const [q1, l1] = table[i];
    if (q <= q1) return l0 + ((q - q0) / (q1 - q0)) * (l1 - l0);
  }
  return table[table.length - 1][1];
}

function sentence(r: R, f: { code: number; bold: number; link: number; localLink: number; url: number }) {
  let s = pick(r, PHRASES);
  if (r() < f.code) s += `，见 \`${pick(r, FILES)}\``;
  if (r() < f.bold) s = `**${pick(r, ['结论：', '原因', '改动', '下一步：'])}**${s}`;
  if (r() < f.localLink) s += `（[${pick(r, FILES)}](${pick(r, FILES)}:${Math.floor(r() * 400)})）`;
  if (r() < f.link) s += `，参考 [说明](https://example.com/docs/${Math.floor(r() * 99)})`;
  if (r() < f.url) s += ` https://example.com/a_b_c/${Math.floor(r() * 99)}`;
  return s + pick(r, ['。', '。', '；', '：', '？']);
}

function table(r: R) {
  const cols = r() < 0.75 ? 3 : r() < 0.8 ? 4 : 5;
  const rows = 3 + Math.floor(r() * r() * 12);
  const cell = () => (r() < 0.25 ? `\`${pick(r, FILES)}\`` : pick(r, PHRASES).slice(0, 4 + Math.floor(r() * 16)));
  const head = Array.from({ length: cols }, () => pick(r, ['项目', '现在', '之后', '原因', '处理', '结果']));
  const lines = [`| ${head.join(' | ')} |`, `|${' --- |'.repeat(cols)}`];
  for (let i = 0; i < rows; i++) lines.push(`| ${Array.from({ length: cols }, cell).join(' | ')} |`);
  return lines.join('\n');
}

function code(r: R) {
  const n = r() < 0.92 ? 2 + Math.floor(r() * 6) : 30 + Math.floor(r() * 90);
  const body = Array.from({ length: n }, () => pick(r, CODE_LINES));
  return `\`\`\`${pick(r, ['ts', 'rust', 'sh', ''])}\n${body.join('\n')}\n\`\`\``;
}

function list(r: R, nested: boolean, f: Parameters<typeof sentence>[1]) {
  const ordered = r() < 0.3;
  const n = 2 + Math.floor(r() * 4);
  const out: string[] = [];
  for (let i = 0; i < n; i++) {
    out.push(`${ordered ? `${i + 1}.` : '-'} ${sentence(r, f)}`);
    if (nested && r() < 0.5) out.push(`   - ${sentence(r, f)}`, `   - ${sentence(r, f)}`);
  }
  return out.join('\n');
}

/** A reply that ends a turn: rates are the share of final replies with each construct. */
export function finalReply(r: R): string {
  const target = lengthAt(FINAL_LENGTHS, r());
  // Whether a reply uses a construct at all is the measured rate; how often within it is not.
  const f = {
    code: r() < 0.63 ? 0.45 : 0,
    bold: r() < 0.71 ? 0.3 : 0,
    link: r() < 0.08 ? 0.3 : 0,
    localLink: r() < 0.14 ? 0.3 : 0,
    url: r() < 0.022 ? 0.2 : 0,
  };
  const listed = r() < 0.7;
  const want = {
    table: target > 500 && r() < 0.235 / 0.55,
    nested: listed && target > 300 && r() < 0.156 / (0.7 * 0.78),
    list: listed,
    heading: target > 400 && r() < 0.08 / 0.6,
    code: target > 200 && r() < 0.049 / 0.8,
    quote: r() < 0.014,
    rule: r() < 0.011,
    image: r() < 0.005,
    citation: r() < 0.016,
  };
  const parts: string[] = [];
  if (want.heading) parts.push(`## ${pick(r, ['结论', '改了什么', '还差什么'])}`);
  parts.push(sentence(r, f));
  if (want.list) parts.push(list(r, want.nested, f));
  if (want.table) parts.push(table(r));
  if (want.code) parts.push(code(r));
  if (want.quote) parts.push(`> ${sentence(r, f)}`);
  if (want.rule) parts.push('---');
  if (want.image) parts.push(`![截图](/tmp/shot_${Math.floor(r() * 9)}.png)`);
  let size = parts.join('\n\n').length;
  while (size < target) {
    const p = !listed || r() < 0.75 ? `${sentence(r, f)}${sentence(r, f)}` : list(r, false, f);
    parts.push(p);
    size += p.length;
  }
  if (want.citation) parts.push('<oai-mem-citation>\n<citation_entries>\nMEMORY.md:1-2|note=[example]\n</citation_entries>\n<rollout_ids>\nexample\n</rollout_ids>\n</oai-mem-citation>');
  return parts.join('\n\n');
}

/** A line between tool calls: mostly one short sentence. */
export function midReply(r: R): string {
  const target = lengthAt(MID_LENGTHS, r());
  const f = { code: r() < 0.2 ? 0.5 : 0, bold: r() < 0.023 ? 0.5 : 0, link: 0, localLink: r() < 0.002 ? 1 : 0, url: 0 };
  let s = sentence(r, f);
  while (s.length < target) s += sentence(r, f);
  return r() < 0.012 ? `${s}\n\n- ${sentence(r, f)}\n- ${sentence(r, f)}` : s;
}

export type Sample = { seq: number; text: string; final: boolean };

/** `count` replies, 31% final as in the logs, or all final. */
export function replies(count: number, seed: number, finalShare = 0.31): Sample[] {
  const r = rng(seed);
  return Array.from({ length: count }, (_, seq) => {
    const final = r() < finalShare;
    return { seq, final, text: final ? finalReply(r) : midReply(r) };
  });
}

/** The longest kind of reply seen: every construct, about 11.7 KB. */
export function longest(seed = 7): string {
  const r = rng(seed);
  const f = { code: 0.4, bold: 0.4, link: 0.1, localLink: 0.1, url: 0.02 };
  const parts: string[] = ['## 结论'];
  while (parts.join('\n\n').length < 11700) {
    parts.push(`${sentence(r, f)}${sentence(r, f)}`, list(r, true, f), table(r));
    if (r() < 0.3) parts.push(code(r));
  }
  return parts.join('\n\n').slice(0, 11700);
}
