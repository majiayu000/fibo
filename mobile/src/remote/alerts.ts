// What deserves a notification on this phone, from the cards it already gets decrypted. Whether a
// card asks for you is the host's answer (`needs_you`, `status::needs_you` on the computer: 先不管
// and hidden conversations are false there); this only turns "it asks now" into "tell them once",
// and applies the switches on the settings page. No relay or push service sees any of it.
// Kept apart from the hub and from React Native so a test can walk it without a phone.
import type { Settings } from '../store/settings';
import { cardTitle } from '../ui/format';
import { statusKey, statusWord } from '../ui/status';
import type { Card, Machine } from './hub';

type Notify = Settings['notify'];
export type Kind = 'approval' | 'waiting' | 'error' | 'done';

/** A notification to put up; a later one with the same `tag` replaces it. `quiet` changes its words without alerting again. */
export type Alert = { tag: string; title: string; body: string; quiet?: true };

export type Memory = {
  /** Per machine, each ask it was seen with: 0 while it asks, else when it stopped (ms). */
  seen: Record<string, Record<string, number>>;
  /** Tags of the notifications up now, to the machine they came from. */
  shown: Record<string, string>;
  /** Of those, what the last alert said and when (ms). Absent in a memory saved by the first build. */
  rang?: Record<string, { kind: Kind; turn: string; at: number }>;
};

export const NO_MEMORY: Memory = { seen: {}, shown: {} };

/**
 * Whether 后台保持连接 is on: `backgroundAlerts`, and only when it is `true`. A phone saved before
 * the field reads as off, and so does one that ran the 11-07 trial build, which saved
 * `keepConnected`: nobody comes back to a foreground service they did not turn on in this build.
 */
export const keepsConnected = (v: Partial<Pick<Settings, 'backgroundAlerts'>>) => v.backgroundAlerts === true;

/**
 * An ask that went away and came back within this is the same ask: the screen reader and the
 * log can disagree for a moment, and a phone should not buzz for that.
 */
export const FLICKER_MS = 30_000;

/** The switch on the settings page that governs this card; null when it does not ask for you. */
export function kindOf(c: Card): Kind | null {
  if (!c.needs_you) return null;
  switch (statusKey(c)) {
    case 'approval':
      return 'approval';
    case 'waiting':
      return 'waiting';
    case 'error':
      return 'error';
    case 'review':
    case 'background':
      return 'done';
    default:
      return null;
  }
}

/** 23:00–08:00 with 免打扰 on, approvals excepted, as the settings page says. */
const hushed = (kind: Kind, notify: Notify, at: Date) => notify.quiet && kind !== 'approval' && (at.getHours() >= 23 || at.getHours() < 8);

/** Names one ask: a second approval in the same turn is a new ask, the same one refreshed is not. */
function askKey(c: Card, kind: Kind, turn: string): string {
  const s = [c.id, turn, kind, c.approval ?? c.waiting ?? ''].join('\n');
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193);
  return (h >>> 0).toString(36);
}

export type StepContext = {
  notify: Notify;
  /** Nobody is told while they are looking at the app, or with 后台保持连接 off. */
  silent: boolean;
  /** With the app's lock on, a notification does not say which conversation. */
  hideBody: boolean;
  at: Date;
};

const same = (a: Record<string, unknown>, b: Record<string, unknown>) => {
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => a[k] === b[k]);
};

/** The next memory, what to put up and what to take down. `memory` itself comes back when nothing changed. */
export function step(memory: Memory, machines: Machine[], ctx: StepContext): { memory: Memory; ring: Alert[]; clear: string[] } {
  const now = ctx.at.getTime();
  const seen: Memory['seen'] = {};
  const shown: Memory['shown'] = {};
  const ring: Alert[] = [];
  const clear: string[] = [];
  const rang: NonNullable<Memory['rang']> = { ...memory.rang };
  const known = new Set(machines.map((m) => m.id));
  for (const [tag, machine] of Object.entries(memory.shown)) {
    if (!known.has(machine)) clear.push(tag);
    else shown[tag] = machine;
  }
  for (const m of machines) {
    const before = memory.seen[m.id];
    // Not online, its cards are what it last said; no list yet, it has said nothing. Neither says
    // anything has changed, and an empty list here would ring everything once the real one comes.
    if (m.status !== 'online' || m.updated === undefined) {
      if (before) seen[m.id] = before;
      continue;
    }
    const next: Record<string, number> = {};
    const live = new Set<string>();
    for (const c of m.cards) {
      const kind = kindOf(c);
      if (!kind) continue;
      // Only to tell one ask from another, never to decide whether it asks.
      const turn = `${c.session ?? ''}\n${c.turn ?? ''}`;
      const key = askKey(c, kind, turn);
      next[key] = 0;
      const tag = `${m.id}/${c.id}`;
      if (ctx.notify[kind]) live.add(tag);
      // The first list from a machine is where things stand, not news: a phone that just turned
      // this on is not told about everything already waiting.
      if (!before || key in before || !ctx.notify[kind] || ctx.silent || hushed(kind, ctx.notify, ctx.at)) continue;
      const last = rang[tag];
      // The hook says it is waiting seconds before the screen says on what: that first 等你回答
      // becoming 等你审批 is the same ask. The tag still up means nobody has answered it.
      const reread = kind === 'approval' && last?.kind === 'waiting' && last.turn === turn && now - last.at < FLICKER_MS && tag in shown;
      ring.push({ tag, title: statusWord(c), body: ctx.hideBody ? '' : cardTitle(c), ...(reread ? { quiet: true as const } : {}) });
      shown[tag] = m.id;
      rang[tag] = { kind, turn, at: now };
    }
    for (const [key, at] of Object.entries(before ?? {})) {
      if (key in next) continue;
      const gone = at || now;
      if (now - gone < FLICKER_MS) next[key] = gone;
    }
    seen[m.id] = next;
    for (const [tag, machine] of Object.entries(shown)) {
      if (machine === m.id && !live.has(tag)) {
        delete shown[tag];
        clear.push(tag);
      }
    }
  }
  const ids = Object.keys(seen);
  const unchanged =
    same(shown, memory.shown) &&
    ids.length === Object.keys(memory.seen).length &&
    ids.every((id) => memory.seen[id] && same(seen[id], memory.seen[id]));
  for (const tag of Object.keys(rang)) if (!(tag in shown)) delete rang[tag];
  return { memory: unchanged ? memory : { seen, shown, rang }, ring, clear };
}

export type AlertDeps = {
  memory: Memory;
  machines: () => Machine[];
  subscribe: (listener: () => void) => () => void;
  context: () => StepContext;
  save: (memory: Memory) => void;
  show: (alert: Alert) => void;
  hide: (tag: string) => void;
};

/** Walks every change of the hub through `step`; returns how to stop. */
export function followAlerts(d: AlertDeps): () => void {
  let memory = d.memory;
  const run = () => {
    const out = step(memory, d.machines(), d.context());
    if (out.memory !== memory) d.save(out.memory);
    memory = out.memory;
    out.clear.forEach(d.hide);
    out.ring.forEach(d.show);
  };
  run();
  return d.subscribe(run);
}
