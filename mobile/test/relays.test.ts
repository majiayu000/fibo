import { expect, test } from 'bun:test';
import { linkText, parseLink, relaysOf } from '../src/remote/link';
import { firstThrough } from '../src/remote/race';

const code = 'A'.repeat(43);
const base = `mooring://pair?v=2&relay=https%3A%2F%2Fa.example.com&code=${code}&device=d1&key=${code}&machine=m&name=n`;

test('a link names more relays with alt=, and one without them reads as before', () => {
  expect(parseLink(base).alts).toBeUndefined();
  expect(relaysOf(parseLink(base))).toEqual(['https://a.example.com']);
  const text = base.replace('&code=', '&alt=https%3A%2F%2Fb.example.com&alt=https%3A%2F%2Fa.example.com&code=');
  const link = parseLink(text);
  expect(link.alts).toEqual(['https://b.example.com']);
  expect(relaysOf(link)).toEqual(['https://a.example.com', 'https://b.example.com']);
  expect(parseLink(linkText(link))).toEqual(link);
  // A phone before 11-26 took the first `relay=` by name; `alt` is not it.
  expect(linkText(link).split('&').find((kv) => kv.startsWith('relay='))).toBe('relay=https%3A%2F%2Fa.example.com');
});

type Fake = { name: string; closed: boolean; close(): void };
const fake = (name: string): Fake => ({ name, closed: false, close() { this.closed = true; } });
const after = <T,>(ms: number, v: T, fail = false) =>
  new Promise<T>((res, rej) => setTimeout(() => (fail ? rej(v) : res(v)), ms));
/** A relay that refuses after `ms`: typed as the relay it would have been, as `firstThrough` takes. */
const down = (ms: number, why: string) => new Promise<Fake>((_, rej) => setTimeout(() => rej(new Error(why)), ms));

test('the first relay through wins and the later one is closed', async () => {
  const slow = fake('slow');
  const won = await firstThrough([() => after(30, slow), () => after(5, fake('fast'))]);
  expect(won.name).toBe('fast');
  await after(40, null);
  expect(slow.closed).toBe(true);
});

test('one relay down does not matter; all down fails with the first relay\'s reason', async () => {
  const won = await firstThrough([() => down(1, 'a down'), () => after(10, fake('b'))]);
  expect(won.name).toBe('b');
  const failed = await firstThrough([() => down(10, 'a down'), () => down(1, 'b down')]).catch((e) => e);
  expect((failed as Error).message).toBe('a down');
});
