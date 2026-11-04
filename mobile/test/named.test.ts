import { expect, test } from 'bun:test';
import { named, UNSURE } from '../src/remote/named';

/** A connection that answers with `answer`, or ends before answering when `drops`. */
function link(answer: (fields: any) => any, drops = false) {
  const seen: any[] = [];
  const l = {
    ended: null as unknown,
    seen,
    async request(_op: string, fields: any = {}) {
      seen.push(fields);
      if (drops) {
        l.ended = { code: 4408 };
        throw new Error('no answer');
      }
      return answer(fields);
    },
  };
  return l;
}

test('a write cut off mid-way goes again on the new connection under the same id', async () => {
  const first = link(() => 'never', true);
  const second = link(() => 'ok');
  let now: any = first;
  setTimeout(() => (now = second), 20);
  expect(await named(() => now, 'send', { text: 'hi' }, 1000, 5)).toBe('ok');
  expect(first.seen[0].request).toBeTruthy();
  expect(second.seen[0].request).toBe(first.seen[0].request);
  expect(second.seen[0].text).toBe('hi');
});

test('the machine saying no is not tried again', async () => {
  const l = link(() => {
    throw new Error('只看');
  });
  await expect(named(() => l, 'keys', {}, 1000, 5)).rejects.toThrow('只看');
  expect(l.seen.length).toBe(1);
});

test('a machine that never comes back leaves the write unsure', async () => {
  await expect(named(() => undefined, 'send', {}, 30, 5)).rejects.toThrow(UNSURE);
});
