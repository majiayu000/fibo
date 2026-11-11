import type { QuotaWindow, QuotaLevel } from '../store/quota';
// Reading what the machine sends into what the screens show: whether an approval is
// dangerous, the options a question offers, a patch's lines, quota windows. Pure, so
// tests run it without React Native.
import { tk, tr } from '../i18n';

/** A screen line without the box Claude Code draws around a prompt. */
const unbox = (line: string) => line.replace(/^\s*[│┃|]\s?/, '').replace(/\s*[│┃|]\s*$/, '');
const filled = (line: string) => line.trim() && !/^[\s╭╮╰╯─━┄┈]+$/.test(line);

/** What an approval asks about, as the machine read it off the screen (`ask`). */
export type Ask = {
  /** What it wants to run or touch; the question itself until the machine has said. */
  subject: string;
  /** Its kind as the prompt titles it: "Bash command", "Edit file". */
  kind: string;
  /** What the Agent says it does: "Create empty second.txt file". */
  note: string;
  /** A file's new contents or a diff. */
  detail: string;
  /** The machine could not read all of it. */
  truncated: boolean;
  /** The machine named the subject. Until then nothing can be judged from it. */
  known: boolean;
  /** What the menu's 「don't ask again」 option grants, as the machine read it; '' when it has none. */
  always: string;
};

/** The reply to `ask` as screens read it; `question` stands in until there is a subject. */
export function askFrom(reply: { kind?: string; subject?: string; note?: string; detail?: string; truncated?: boolean; always?: string } | null, question: string): Ask {
  const subject = reply?.subject ?? '';
  return {
    subject: subject || question,
    kind: reply?.kind ?? '',
    note: reply?.note ?? '',
    detail: reply?.detail ?? '',
    truncated: !!reply?.truncated,
    known: !!subject,
    always: reply?.always ?? '',
  };
}

/** Whether allowing it asks once more: a dangerous command, or one nobody could read. */
export const asksTwice = (ask: Pick<Ask, 'subject' | 'known' | 'truncated'>) => !ask.known || ask.truncated || dangerous(ask.subject);

/** What a folder-trust question says, in the person's words: what trusting it allows. */
export const trustWords = (agent: string) => ({
  ask: tr('{agent} 问：信任这个文件夹吗？', { agent: agent || 'Agent' }),
  means: tr('信任后，它能读、改、运行这个文件夹里的文件。这个决定会记在电脑上。不信任的话，它不会在这里工作。'),
});

/** What a conversation read only from its log's end says about what is missing. */
export const partialWords = (messages: number) =>
  messages
    ? tr('更早的消息没有读进来：电脑第一次看到这段对话时，只读了日志的最后一段。')
    : tr('这段日志电脑只读了最后一段，里面没有消息。之后的新消息会显示，更早的要回电脑上看。');

/** What a conversation with older lines still unread says, on a computer that can read them. */
export const readBackWords = (reading: boolean, failed: string) =>
  reading
    ? tr('正在读更早的消息…')
    : failed
      ? tr('{why}。点这里再试一次', { why: failed })
      : tr('更早的消息还在电脑的日志里。继续往上翻，或者点这里读');

/** Commands worth a second confirmation: pushing, deleting, deploying, publishing. */
const DANGER = [
  /\bgit\s+push\b/,
  /\bgit\s+reset\s+--hard\b/,
  /\bgit\s+clean\b/,
  /(^|[\s;&|(])rm\s/,
  /\brmdir\b/,
  /\bdeploy\b/i,
  /\bpublish\b/,
  /\bkubectl\s+(apply|delete|rollout)\b/,
  /\bterraform\s+(apply|destroy)\b/,
  /\bdrop\s+(table|database)\b/i,
  /--force\b|\s-f\b/,
  /\bsudo\b/,
  /\bdocker\s+push\b/,
];

export const dangerous = (cmd: string) => DANGER.some((r) => r.test(cmd));

export type Choice = { key: string; label: string };

/** Numbered options a question shows ("❯ 1. Yes"), for one-tap answers. */
export function choices(text: string | null | undefined): Choice[] {
  if (!text) return [];
  const out: Choice[] = [];
  for (const raw of text.split('\n')) {
    const m = unbox(raw).match(/^\s*[❯>›]?\s*(\d)[.)]\s+(.+?)\s*$/);
    if (m && !out.some((c) => c.key === m[1])) out.push({ key: m[1], label: m[2] });
  }
  return out.length >= 2 ? out : [];
}

/** The question without its numbered options. */
export const questionText = (text: string) =>
  text
    .split('\n')
    .map(unbox)
    .filter((l) => !/^\s*[❯>›]?\s*\d[.)]\s+/.test(l) && filled(l))
    .join('\n')
    .trim();

export type DiffLine =
  | { kind: 'hunk'; text: string }
  | { kind: 'add' | 'del' | 'ctx'; no: number; text: string };

/** A file's unified diff as numbered lines; deletions carry the old number, others the new. */
export function diffLines(patch: string): DiffLine[] {
  const out: DiffLine[] = [];
  let old = 0;
  let neu = 0;
  let inHunk = false;
  for (const line of patch.split('\n')) {
    const h = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/);
    if (h) {
      old = Number(h[1]);
      neu = Number(h[2]);
      inHunk = true;
      out.push({ kind: 'hunk', text: line });
      continue;
    }
    if (!inHunk) continue;
    if (line.startsWith('\\')) continue;
    if (line.startsWith('+')) out.push({ kind: 'add', no: neu++, text: line.slice(1) });
    else if (line.startsWith('-')) out.push({ kind: 'del', no: old++, text: line.slice(1) });
    else if (line.startsWith(' ') || line === '') {
      if (line === '' && out.at(-1)?.kind === 'hunk') continue;
      out.push({ kind: 'ctx', no: neu++, text: line.slice(1) });
      old++;
    }
  }
  // A trailing empty context line is the patch's final newline.
  const last = out.at(-1);
  if (last && last.kind === 'ctx' && last.text === '') out.pop();
  return out;
}

/** "5 小时窗口", "本周", or the window's length. */
export function windowName(minutes: number): string {
  if (minutes === 300) return tr('5 小时窗口');
  if (minutes === 10080) return tr('本周');
  if (minutes === 1440) return tr('今天');
  if (minutes % 1440 === 0) return tr('{d} 天', { d: minutes / 1440 });
  if (minutes % 60 === 0) return tr('{h} 小时', { h: minutes / 60 });
  return tr('{minutes} 分钟', { minutes });
}

/** 「5 小时窗口」 is long for a line; the other windows' names are short already. Mid-line, so
 * English starts it lower-case: "Codex this week 97%", not "Codex This week 97%". */
export function shortWindow(minutes: number): string {
  const name = minutes === 300 ? tr('5 小时') : windowName(minutes);
  return name.charAt(0).toLowerCase() + name.slice(1);
}

const WEEKDAYS = [tk('周日'), tk('周一'), tk('周二'), tk('周三'), tk('周四'), tk('周五'), tk('周六')];

/** Same boundary as core Window::reset; zero means the reset time is unknown. */
export const quotaReset = (resetsAt: number, now = Date.now() / 1000) => resetsAt !== 0 && resetsAt <= now;

/** A past reading is not a measurement of the new window, even when it used to be full. */
export function quotaWindowView(w: QuotaWindow, now = Date.now() / 1000) {
  const reset = quotaReset(w.resets_at, now);
  return {
    used: reset ? 0 : w.used,
    level: reset ? undefined : w.level,
    text: reset ? tr('已重置') : `${w.used}%`,
    hint: reset ? tr('等下次使用时更新') : resetText(w.resets_at, now),
  };
}

/** "1 时 40 分后重置" within a day, else the weekday; empty when the time is unknown or past. */
export function resetText(resetsAt: number, now = Date.now() / 1000): string {
  if (!resetsAt || resetsAt <= now) return '';
  const s = resetsAt - now;
  if (s < 3600) return tr('{m} 分后重置', { m: Math.max(1, Math.round(s / 60)) });
  if (s < 86400) {
    const minutes = Math.round(s / 60);
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return m ? tr('{h} 时 {m} 分后重置', { h, m }) : tr('{h} 时后重置', { h });
  }
  return tr('{day}重置', { day: tr(WEEKDAYS[new Date(resetsAt * 1000).getDay()]) });
}

/** How full a window is, as the gauge colours it: under half, past half, past 80%. */


/** A stretch of time as people say it: 「45 秒」「12 分钟」「1 小时 20 分钟」. */
export function span(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return tr('{secs} 秒', { secs: s });
  const m = Math.floor(s / 60);
  if (m < 60) return tr('{minutes} 分钟', { minutes: m });
  const h = Math.floor(m / 60);
  return m % 60 ? tr('{h} 小时 {m} 分钟', { h, m: m % 60 }) : tr('{h} 小时', { h });
}

/** A running turn's time beside its state word; nothing in its first minute. */
export const runningFor = (since: number | null | undefined, now: number) => (since && now - since >= 60 ? span(now - since) : '');

/** 12800000 as 「12.8M」. */
export function compact(n: number): string {
  const one = (x: number) => (x >= 100 ? String(Math.round(x)) : String(Math.round(x * 10) / 10));
  if (n >= 1e9) return `${one(n / 1e9)}B`;
  if (n >= 1e6) return `${one(n / 1e6)}M`;
  if (n >= 1e3) return `${one(n / 1e3)}K`;
  return String(n);
}

/** A model's name as a person says it: `claude-opus-5-5` is 「Opus 5.5」; others as they are. */
export function modelName(model: string): string {
  const m = model.match(/^claude-([a-z]+)-(\d+)(?:-(\d{1,2}))?(?:-\d{8})?$/);
  if (!m) return model;
  const name = m[1][0].toUpperCase() + m[1].slice(1);
  return m[3] ? `${name} ${m[2]}.${m[3]}` : `${name} ${m[2]}`;
}

export type Check = { command: string; counts?: string; ok: boolean; at: number; stale: boolean };

/** What the conversation's last test commands said, from their exit codes. */
export function checkLine(checks: Check[] | undefined): { text: string; failed: boolean; stale: boolean } | null {
  if (!checks?.length) return null;
  const word = (c: Check) => (c.ok ? tr('{command} 通过', { command: c.command }) : tr('{command} 没通过', { command: c.command }));
  const fresh = checks.filter((c) => !c.stale);
  // Changed since: the old results say nothing about the files now.
  if (!fresh.length) return { text: tr('{check}，之后又改过', { check: word(checks[0]) }), failed: !checks[0].ok, stale: true };
  return { text: fresh.map(word).join(' · '), failed: fresh.some((c) => !c.ok), stale: false };
}

/** The relay's host, as 「连接」 shows it; a private address reads as the local network. */
export function relayPlace(relay: string): { host: string; lan: boolean } {
  const host = relay.replace(/^\w+:\/\//, '').replace(/\/.*$/, '');
  const name = host.replace(/:\d+$/, '').replace(/^\[|\]$/g, '');
  const lan =
    /^(127\.|10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|169\.254\.)/.test(name) ||
    name === 'localhost' ||
    name.endsWith('.local') ||
    name === '::1';
  return { host, lan };
}

/** A directory as breadcrumbs: `~` for the home directory, each step with its full path. */
export function trail(path: string, home: string): { label: string; path: string }[] {
  const inHome = !!home && (path === home || path.startsWith(`${home}/`));
  const out = [inHome ? { label: '~', path: home } : { label: '/', path: '/' }];
  let at = inHome ? home : '';
  for (const seg of (inHome ? path.slice(home.length) : path).split('/').filter(Boolean)) {
    at += `/${seg}`;
    out.push({ label: seg, path: at });
  }
  return out;
}

/** What a computer's part of the quota page shows, given what is known about it. */
export function quotaView(online: boolean, asks: boolean, saved: unknown[] | null) {
  if (!online) return saved ? 'gone' : 'never';
  if (!asks) return 'unsupported';
  return saved ? 'live' : 'loading';
}

type Reading = { agent: string; agent_name?: string; windows: QuotaWindow[] };
export type Tight = { machine: string; agent: string; agent_name?: string; minutes: number; used: number; level?: QuotaLevel };

/** Each Agent's most used window on each computer, fullest first. Never added up: see `app/quota.tsx`. A window past its reset says nothing. */
export function tightWindows(readings: { machine: string; agents: Reading[] | null }[], now = Date.now() / 1000): Tight[] {
  const out: Tight[] = [];
  for (const { machine, agents } of readings) {
    for (const a of agents ?? []) {
      // Old hosts/cached readings do not supply severity or a shared ordering.
      if (a.windows.some((w) => !w.level)) continue;
      const top = a.windows.find((w) => !quotaReset(w.resets_at, now));
      if (!top) continue;
      out.push({ machine, agent: a.agent, agent_name: a.agent_name, minutes: top.minutes, used: top.used, level: top.level });
    }
  }
  return out.sort((x, y) => y.used - x.used);
}

export type PathChip =
  | { kind: 'level'; label: string; path: string; root: boolean; current: boolean }
  | { kind: 'more'; hidden: { label: string; path: string }[] };

/**
 * A `trail` as the folder chips above the list: the root, the last two levels, and the ones
 * between folded into one 「… N 级」 when there are more than four. The root `/` reads 电脑:
 * as a chip next to the separators, a bare `/` read as 「/ / private」 (2026-11-09).
 */
export function pathChips(crumbs: { label: string; path: string }[]): PathChip[] {
  const level = (c: { label: string; path: string }, i: number): PathChip => ({
    kind: 'level',
    label: i === 0 && c.label === '/' ? tr('电脑') : c.label,
    path: c.path,
    root: i === 0,
    current: i === crumbs.length - 1,
  });
  if (crumbs.length <= 4) return crumbs.map(level);
  return [level(crumbs[0], 0), { kind: 'more', hidden: crumbs.slice(1, -2) }, ...crumbs.slice(-2).map((c, j) => level(c, crumbs.length - 2 + j))];
}

/** What the 改动 page calls itself and its empty state, by what the computer sent: a phone
 * paired as 只看 or 能答审批 gets `summary`, this turn's file names and counts only. */
export function diffWords(d: { summary?: boolean } | null) {
  return d?.summary
    ? { title: tr('这一轮改了哪些文件'), empty: tr('这一轮还没改文件'), note: tr('这台手机的权限只显示文件名和行数') }
    : { title: tr('未提交的改动'), empty: tr('没有未提交的改动'), note: '' };
}
