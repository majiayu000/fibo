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


test('a restored outbox item keeps its request id after the previous call lost its receipt', async () => {
  const stored = JSON.stringify({ id: 'persisted-message', terminal: 't', text: 'hello' });
  const applied = new Set<string>();
  const seen: string[] = [];
  let executions = 0;
  function accept(fields: any) {
    seen.push(fields.request);
    if (!applied.has(fields.request)) {
      applied.add(fields.request);
      executions++;
    }
    return 'ok';
  }
  const first = {
    ended: null as unknown,
    async request(_op: string, fields: any) {
      accept(fields);
      this.ended = { code: 1006 };
      throw new Error('receipt lost');
    },
  };
  const fields = () => {
    const item = JSON.parse(stored);
    return { terminal: item.terminal, text: item.text, enter: true, request: item.id };
  };
  await expect(named(() => first, 'send', fields(), 20, 1)).rejects.toThrow(UNSURE);
  // A fresh invocation and link, reconstructed only from what the outbox persisted.
  const second = link(accept);
  expect(await named(() => second, 'send', fields())).toBe('ok');
  expect(seen).toEqual(['persisted-message', 'persisted-message']);
  expect(executions).toBe(1);
});

test('independent writes without an outbox id still receive distinct ids', async () => {
  const l = link(() => 'ok');
  await named(() => l, 'send', { text: 'one' });
  await named(() => l, 'send', { text: 'two' });
  expect(l.seen[0].request).not.toBe(l.seen[1].request);
});
