// What a `cards` reply means for the cards a phone has: most refreshes change one or two of them,
// so the computer sends those and the order only when it moved (crates/fibo-host/src/remote/cards.rs).
// Kept apart from the hub so a test can walk it without a connection.

export type CardsReply<C> = { rev: number; order?: string[]; changed?: C[] };

/** The cards after `reply`, or null if it names one this phone does not have: then ask for all. */
export function patch<C extends { id: string }>(have: C[], reply: CardsReply<C>): C[] | null {
  const fresh = reply.changed ?? [];
  // Nothing changed: the same array, so a screen that compares it has nothing to redo.
  if (!fresh.length && !reply.order) return have;
  const known = new Map(have.map((c) => [c.id, c]));
  for (const c of fresh) known.set(c.id, c);
  const out: C[] = [];
  for (const id of reply.order ?? have.map((c) => c.id)) {
    const c = known.get(id);
    if (!c) return null;
    out.push(c);
  }
  return out;
}

/** The least time from a burst of events to the refresh it asks for. */
export const MERGE_MS = 150;
/**
 * The least time between refreshes asked for by events, so a machine busy all minute costs
 * less than one refresh per event; an event's change still shows within this and a round trip.
 */
export const GAP_MS = 600;

/** How long an event arriving at `now` waits, the last refresh having gone at `last`. */
export const refreshWait = (now: number, last: number) => Math.max(MERGE_MS, last + GAP_MS - now);
