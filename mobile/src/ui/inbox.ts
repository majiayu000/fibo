// Which list of the inbox a conversation goes in: the inbox, 稍后提醒 or 已处理.
import type { Card, Machine } from '../remote/hub';
import { rank } from './status';

export type Item = { m: Machine; c: Card };

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
      const asks = Boolean(c.needs_you || c.approval || c.phase === 'waiting');
      if (c.outcome === 'done' && !asks) done.push(item);
      else if (c.outcome === 'deferred' && !asks) later.push(item);
      else inbox.push(item);
    }
  }
  const newest = (a: Item, b: Item) => (b.c.updated ?? 0) - (a.c.updated ?? 0);
  inbox.sort((a, b) => rank(a.c) - rank(b.c) || newest(a, b));
  later.sort(newest);
  done.sort(newest);
  return { inbox, later, done };
}
