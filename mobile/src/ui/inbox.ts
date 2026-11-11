// Where the inbox puts things, and when it may move them. A tap lands on what is under the
// finger by the time it comes down: what asks you must not change places on its own, or
// 「允许」 lands on another card. Pure, so tests run it without React Native.
import { tr } from '../i18n';
import type { Card, Machine } from '../remote/hub';
import { cardTitle } from './format';
import { rank, spaceName, statusKey, statusWord } from './status';

export type Item = { m: Machine; c: Card };

export const itemKey = ({ m, c }: Item) => `${m.id}/${c.id}`;

/** On a computer that can answer now, and asking: an approval, a question, a folder to trust. */
const asks = ({ m, c }: Item) => m.status === 'online' && rank(c) <= 1;

const newest = (a: Item, b: Item) => (b.c.updated ?? 0) - (a.c.updated ?? 0);

/** Conversations sorted into the inbox, 稍后提醒 and 已处理. */
export function sort(machines: Machine[]) {
  const inbox: Item[] = [];
  const later: Item[] = [];
  const done: Item[] = [];
  for (const m of machines) {
    for (const c of m.cards) {
      if (!c.agent) continue;
      const item = { m, c };
      // The host's 要你处理 (a 稍后提醒 whose hour is up counts), or something new to
      // answer, brings a settled conversation back.
      const answers = Boolean(c.needs_you || c.approval || c.phase === 'waiting');
      if (c.outcome === 'abandoned' && !answers) continue;
      if (c.outcome === 'done' && !answers) done.push(item);
      else if (c.outcome === 'deferred' && !answers) later.push(item);
      else inbox.push(item);
    }
  }
  inbox.sort((a, b) => rank(a.c) - rank(b.c) || newest(a, b));
  later.sort(newest);
  done.sort(newest);
  return { inbox, later, done };
}

export type Arranged = {
  /** The keys of `pinned`, to hand to the next `arrange`. */
  pins: string[];
  /** What asks you: in the order it appeared, whatever else changes. */
  pinned: Item[];
  /** The rest of the inbox: errors, what runs, the rest; newest first within. */
  rest: Item[];
  later: Item[];
  done: Item[];
  offline: Machine[];
};

/**
 * The inbox from the machines' cards, as `sort` lists them. `pins` is the order last time:
 * what still asks keeps its place and what is new goes after it, so nothing pushes a card
 * down. Several new at once go oldest first, the order they would have come in one by one.
 */
export function arrange(machines: Machine[], pins: string[]): Arranged {
  const { inbox, later, done } = sort(machines);
  const asking = new Map(inbox.filter(asks).map((i) => [itemKey(i), i]));
  const kept = pins.filter((k) => asking.has(k));
  const had = new Set(kept);
  const fresh = [...asking.values()]
    .filter((i) => !had.has(itemKey(i)))
    .sort((a, b) => (a.c.updated ?? 0) - (b.c.updated ?? 0) || itemKey(a).localeCompare(itemKey(b)))
    .map(itemKey);
  const order = [...kept, ...fresh];
  return {
    pins: order,
    pinned: order.map((k) => asking.get(k)!),
    rest: inbox.filter((i) => !asking.has(itemKey(i))),
    later,
    done,
    offline: machines.filter((m) => m.status !== 'online'),
  };
}

/** A 稍后提醒 whose hour is up: you asked to be reminded, so it does not go quietly into a fold. */
const reminder = ({ c }: Item) => statusKey(c) === 'review' && c.outcome === 'deferred';

/** What is yours to act on now, on a computer that can hear you. Finished work to check is not. */
const acts = (i: Item) => i.m.status === 'online' && (rank(i.c) <= 2 || reminder(i));

/** One entry of the space filter; `name` null is 全部. `count` is its inbox, `acts` what of that is yours to act on. */
export type Chip = { name: string | null; count: number; acts: number };

export type Shelf = {
  /** The space shown, null for all. A saved choice whose space has gone reads as all. */
  filter: string | null;
  /** Empty with fewer than two spaces. By name, so a chip never moves under a finger. */
  chips: Chip[];
  pinned: Item[];
  /** Errors and reminders that came back. */
  ahead: Item[];
  /** 等你验收: it folds into one line, still in the inbox. */
  review: Item[];
  /** What runs: nothing to do here yet, so it shows `RUNNING_PREVIEW` at first and does not push 等你验收 away. */
  running: Item[];
  /** What rests: below the fold line. */
  behind: Item[];
  later: Item[];
  done: Item[];
  /** What is yours to act on in the spaces the filter hides. Never hidden without this. */
  elsewhere: number;
  /** Over all spaces: what is yours to act on, and all the host counts as yours, 等你验收 included. */
  acts: number;
  waiting: number;
  /** Nothing of the inbox in the space shown; folded cards count as something. */
  empty: boolean;
};

/**
 * What the screen lays out from `arrange`. It takes the result and leaves `pins` alone: the
 * filter and the fold decide what shows, never in what order what asks you stays.
 */
export function shelve(a: Arranged, selected: string | null): Shelf {
  const inbox = [...a.pinned, ...a.rest];
  const name = (i: Item) => spaceName(i.m, i.c);
  const names = [...new Set([...inbox, ...a.later, ...a.done].map(name))].sort((x, y) => x.localeCompare(y));
  const chip = (n: string | null, items: Item[]): Chip => ({ name: n, count: items.length, acts: items.filter(acts).length });
  const chips = names.length < 2 ? [] : [chip(null, inbox), ...names.map((n) => chip(n, inbox.filter((i) => name(i) === n)))];
  const filter = selected !== null && chips.some((chip) => chip.name === selected) ? selected : null;
  const here = (i: Item) => filter === null || name(i) === filter;
  const rest = a.rest.filter(here);
  const first = rest.filter((i) => rank(i.c) <= 2);
  return {
    filter,
    chips,
    pinned: a.pinned.filter(here),
    ahead: [...first, ...rest.filter(reminder)],
    review: rest.filter((i) => rank(i.c) === 4 && !reminder(i)),
    running: rest.filter((i) => rank(i.c) === 3),
    behind: rest.filter((i) => rank(i.c) > 4),
    later: a.later.filter(here),
    done: a.done.filter(here),
    elsewhere: inbox.filter((i) => !here(i) && acts(i)).length,
    acts: inbox.filter(acts).length,
    waiting: inbox.filter((i) => acts(i) || (i.m.status === 'online' && i.c.needs_you)).length,
    empty: !inbox.some(here),
  };
}

/** The fold's line, in the word the host gives these cards. */
export const foldLabel = (items: Item[]) => {
  const word = items[0] ? statusWord(items[0].c) : '';
  return word ? `${word} · ${items.length}` : `${items.length}`;
};

/** How long after a card appears or changes a tap on its buttons does nothing. */
export const SETTLE_MS = 600;
/** How long the list must be still after scrolling before it may move again. */
export const SCROLL_QUIET_MS = 150;
/** A finger that long down is a lost touch-end: a frozen inbox would hide what needs you. */
export const TOUCH_LIMIT_MS = 10_000;

/** What the content under a card's buttons was, and when it last changed. */
export type Settle = { key: string; at: number };

/** `key` says what the buttons belong to and where they are: any change starts the wait again. */
export const settleNext = (last: Settle | null, key: string, now: number): Settle => (last?.key === key ? last : { key, at: now });

/** Milliseconds until a tap counts; 0 once it does. */
export const settleLeft = (s: Settle, now: number, ms = SETTLE_MS) => Math.max(0, s.at + ms - now);

export type Hold = { touching: boolean; touchedAt: number; scrolledAt: number };

export const HOLD_NONE: Hold = { touching: false, touchedAt: 0, scrolledAt: 0 };

/** Milliseconds the list stays as it is: a finger down or a scroll not yet still. 0 when it may move. */
export function holdLeft(h: Hold, now: number): number {
  const touch = h.touching ? h.touchedAt + TOUCH_LIMIT_MS - now : 0;
  const scroll = h.scrolledAt + SCROLL_QUIET_MS - now;
  return Math.max(0, touch, scroll);
}

/**
 * The line under the Fibo mark: what the space shown holds now, most urgent first, two kinds
 * at most (「1 个等你回答 · 2 个等你验收」). The words are the computer's own status labels.
 * `urgent` when something cannot go on without you.
 */
export function pulse(s: Shelf): { text: string; urgent: boolean } {
  const items = [...s.pinned, ...s.ahead, ...s.running, ...s.review].filter((i) => i.m.status === 'online');
  const groups = new Map<string, { rank: number; n: number }>();
  for (const { c } of items) {
    const word = statusWord(c);
    if (!word) continue;
    const g = groups.get(word) ?? { rank: rank(c), n: 0 };
    g.rank = Math.min(g.rank, rank(c));
    g.n += 1;
    groups.set(word, g);
  }
  // In the list's order: what asks you, then 等你验收 above 在跑 (docs/decisions/2026-11-08-review-above-running.md).
  const order = (r: number) => (r === 4 ? 3 : r === 3 ? 4 : r);
  const top = [...groups].sort((a, b) => order(a[1].rank) - order(b[1].rank) || b[1].n - a[1].n).slice(0, 2);
  if (!top.length) return { text: tr('都歇着'), urgent: false };
  return { text: top.map(([word, g]) => tr('{n} 个{word}', { n: g.n, word })).join(' · '), urgent: top[0][1].rank <= 2 };
}

/** How many of what runs show before 「看全部」: on a real phone 12 pushed everything else two screens down. */
export const RUNNING_PREVIEW = 4;

export const runningShown = <T>(items: T[], all: boolean) => (all ? items : items.slice(0, RUNNING_PREVIEW));

type Linked = Pick<Machine, 'name' | 'status'>;

/** The line under the title. `alert` when no computer can hear you or one refused this phone. */
export function connectionLine(machines: Linked[]): { text: string; alert: boolean } {
  const online = machines.filter((m) => m.status === 'online');
  const off = machines.length - online.length;
  if (machines.length === 0) return { text: '', alert: false };
  if (online.length === 0) return { text: machines.length === 1 ? tr('{name}不在线', { name: machines[0].name || tr('电脑') }) : tr('电脑都不在线'), alert: true };
  const here = online.length === 1 ? tr('{name} 在线', { name: online[0].name || tr('电脑') }) : tr('{n} 台电脑在线', { n: online.length });
  return { text: off > 0 ? tr('{here} · {n} 台离线', { here, n: off }) : here, alert: machines.some((m) => m.status === 'refused') };
}

/** Computers that take a row on the home screen; the rest are counted in the line and listed under 我的. */
export function offlineRows<T extends Linked>(machines: T[]): T[] {
  const off = machines.filter((m) => m.status !== 'online');
  return off.length === machines.length ? off : off.filter((m) => m.status === 'refused');
}

/** The computer's name beside each conversation only when it tells conversations apart. */
export const namesComputer = (machines: Linked[]) => machines.filter((m) => m.status === 'online').length > 1;

/** What 撤销 puts back: where the conversation was before it was swiped. */
export const undoTo = (c: Pick<Card, 'outcome'>): 'done' | 'deferred' | 'abandoned' | 'open' =>
  c.outcome === 'done' || c.outcome === 'deferred' || c.outcome === 'abandoned' ? c.outcome : 'open';

/** Every conversation, 稍后提醒 and 已处理 too, narrowed to a space and to words in its title, space or directory. */
export function searchCards(a: Arranged, query: string, space: string | null): Item[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return [...a.pinned, ...a.rest, ...a.later, ...a.done].filter((i) => {
    const name = spaceName(i.m, i.c);
    if (space !== null && name !== space) return false;
    const hay = [cardTitle(i.c), name, i.c.cwd, i.c.branch].filter(Boolean).join(' ').toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
