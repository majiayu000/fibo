// Messages the phone missed while it was away: the newest page alone would leave a hole
// between what it had and that page, which nothing ever fills (polling asks only for what
// comes after the newest, scrolling up only for what came before the oldest).

/** One chat message as the machine sends it; only `seq` matters here. */
export type Seq = { seq: number };
/** Asks the machine for the messages after `after`, oldest first, at most `limit`. */
export type After<M extends Seq> = (after: number, limit: number) => Promise<M[]>;

/** The page size for catching up; the machine caps a page at 500. */
export const CATCH_UP_PAGE = 500;

/**
 * Everything after `had` and before `newest` (the first seq of the newest page), read a
 * page at a time. Nothing when there is no hole: the phone had nothing, or the newest
 * page already starts right where it stopped.
 */
export async function catchUp<M extends Seq>(
  after: After<M>,
  had: number,
  newest: number | undefined,
  more: boolean,
): Promise<M[]> {
  if (!had || newest === undefined || newest <= had || !more) return [];
  const out: M[] = [];
  let from = had;
  for (;;) {
    const page = await after(from, CATCH_UP_PAGE);
    const inHole = page.filter((m) => m.seq < newest);
    out.push(...inHole);
    const last = page.at(-1)?.seq;
    if (last === undefined || last >= newest || page.length < CATCH_UP_PAGE || last <= from) {
      return out;
    }
    from = last;
  }
}
