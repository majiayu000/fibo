// The hole a phone away for a while would leave between what it had and the newest page.
import { describe, expect, test } from 'bun:test';
import { CATCH_UP_PAGE, catchUp } from '../src/remote/gap';

/** A machine holding messages 1..n (seqs with gaps, as rowids have). */
function machine(n: number) {
  const all = Array.from({ length: n }, (_, i) => ({ seq: (i + 1) * 2 }));
  const asked: number[] = [];
  const after = async (from: number, limit: number) => {
    asked.push(from);
    return all.filter((m) => m.seq > from).slice(0, limit);
  };
  return { all, asked, after };
}

describe('catching up after being away', () => {
  test('fills the hole between what it had and the newest page', async () => {
    const m = machine(200);
    const had = m.all[49].seq; // saw 50
    const newest = m.all.slice(-50); // the newest page: 151..200
    const got = await catchUp(m.after, had, newest[0].seq, true);
    expect(got.map((x) => x.seq)).toEqual(m.all.slice(50, 150).map((x) => x.seq));
  });

  test('pages through a long absence', async () => {
    const m = machine(2 * CATCH_UP_PAGE + 300);
    const had = m.all[9].seq;
    const newest = m.all.at(-50)!.seq;
    const got = await catchUp(m.after, had, newest, true);
    expect(got.length).toBe(m.all.length - 10 - 50);
    expect(m.asked.length).toBe(3);
  });

  test('asks nothing when there is no hole', async () => {
    const m = machine(80);
    expect(await catchUp(m.after, 0, 10, true)).toEqual([]); // had nothing
    expect(await catchUp(m.after, 100, 100, true)).toEqual([]); // already up to date
    expect(await catchUp(m.after, 100, 102, false)).toEqual([]); // the page reaches back
    expect(m.asked).toEqual([]);
  });
});
