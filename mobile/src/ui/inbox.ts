// Where the inbox puts things, and when it may move them. A tap lands on what is under the
// finger by the time it comes down: what asks you must not change places on its own, or
// 「允许」 lands on another card. Pure, so tests run it without React Native.
import type { Card, Machine } from '../remote/hub';
import { rank } from './status';

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
      if (!c.agent || c.phase === 'shell') continue;
      const item = { m, c };
      // The host's 要你处理 (a 稍后提醒 whose hour is up counts), or something new to
      // answer, brings a settled conversation back.
      const answers = Boolean(c.needs_you || c.approval || c.phase === 'waiting');
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
