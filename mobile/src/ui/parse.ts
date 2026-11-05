// Reading what the machine sends into what the screens show: the command an approval
// asks about, the options a question offers, a patch's lines, quota windows. Pure, so
// tests run it without React Native.

/** A screen line without the box Claude Code draws around a prompt. */
const unbox = (line: string) => line.replace(/^\s*[│┃|]\s?/, '').replace(/\s*[│┃|]\s*$/, '');
const filled = (line: string) => line.trim() && !/^[\s╭╮╰╯─━┄┈]+$/.test(line);

export type Ask = {
  /** What it wants to run or touch: a command, a file. */
  subject: string;
  /** Its kind as the prompt titles it: "Bash command", "Edit file". */
  kind: string;
};

/**
 * The approval on a terminal's last lines: the title line ("Bash command"), the first
 * line after it is the subject. `question` is the card's approval, the fallback subject.
 */
export function readAsk(screen: string, question: string): Ask {
  const lines = screen.split('\n').map(unbox);
  const title = lines.findIndex((l) => /^\s*(Bash( command)?|Run|Edit( file)?|Write( file)?|Create( file)?|Read( file)?|Fetch|WebFetch|MCP|Tool use|Command)\b.*$/i.test(l.trim()) && l.trim().length < 40);
  if (title >= 0) {
    const rest = lines.slice(title + 1).filter(filled);
    const subject = rest[0]?.trim();
    if (subject && !/do you want|proceed\?|^❯|^>/i.test(subject)) {
      return { subject, kind: lines[title].trim() };
    }
  }
  // No title: the last filled line above the question.
  const q = lines.findIndex((l) => /do you want|proceed\?|allow/i.test(l));
  const above = q > 0 ? lines.slice(0, q).filter(filled) : [];
  return { subject: above.at(-1)?.trim() || question, kind: '' };
}

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
  if (minutes === 300) return '5 小时窗口';
  if (minutes === 10080) return '本周';
  if (minutes === 1440) return '今天';
  if (minutes % 1440 === 0) return `${minutes / 1440} 天`;
  if (minutes % 60 === 0) return `${minutes / 60} 小时`;
  return `${minutes} 分钟`;
}

const WEEKDAYS = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

/** "1 时 40 分后重置" within a day, else the weekday; empty when the time is unknown or past. */
export function resetText(resetsAt: number, now = Date.now() / 1000): string {
  if (!resetsAt || resetsAt <= now) return '';
  const s = resetsAt - now;
  if (s < 3600) return `${Math.max(1, Math.round(s / 60))} 分后重置`;
  if (s < 86400) {
    const minutes = Math.round(s / 60);
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return m ? `${h} 时 ${m} 分后重置` : `${h} 时后重置`;
  }
  return `${WEEKDAYS[new Date(resetsAt * 1000).getDay()]}重置`;
}

/** How full a window is, as the gauge colours it: under half, past half, past 80%. */
export const level = (used: number): 'low' | 'mid' | 'high' => (used > 80 ? 'high' : used >= 50 ? 'mid' : 'low');

/** A stretch of time as people say it: 「45 秒」「12 分钟」「1 小时 20 分钟」. */
export function span(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} 分钟`;
  return m % 60 ? `${Math.floor(m / 60)} 小时 ${m % 60} 分钟` : `${Math.floor(m / 60)} 小时`;
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
  const word = (c: Check) => `${c.command} ${c.ok ? '通过' : '没通过'}`;
  const fresh = checks.filter((c) => !c.stale);
  // Changed since: the old results say nothing about the files now.
  if (!fresh.length) return { text: `${word(checks[0])}，之后又改过`, failed: !checks[0].ok, stale: true };
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
