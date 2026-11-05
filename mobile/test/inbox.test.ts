// The inbox reads 要你处理 off the host, so what the host brings back shows here too.
import { describe, expect, test } from 'bun:test';
import type { Card, Machine } from '../src/remote/hub';
import { sort } from '../src/ui/inbox';

const card = (o: Partial<Card>): Card => ({ id: 'c', space: 's', tab: 't', cwd: '/', phase: 'idle', agent: 'claude', session: 'x', ...o });
const machine = (cards: Card[]) => ({ id: 'm', cards }) as unknown as Machine;
const where = (c: Card) => {
  const { inbox, later, done } = sort([machine([c])]);
  return inbox.length ? 'inbox' : later.length ? 'later' : done.length ? 'done' : 'none';
};

describe('inbox lists', () => {
  test('a 稍后提醒 whose hour is up comes back when the host says it needs you', () => {
    expect(where(card({ outcome: 'deferred', status: 'review', needs_you: true }))).toBe('inbox');
  });

  test('a 稍后提醒 still waiting its hour stays put', () => {
    expect(where(card({ outcome: 'deferred', status: 'review', needs_you: false }))).toBe('later');
  });

  test('settled conversations stay settled unless something asks', () => {
    expect(where(card({ outcome: 'done', status: 'closed', needs_you: false }))).toBe('done');
    expect(where(card({ outcome: 'done', phase: 'waiting', needs_you: false }))).toBe('inbox');
    expect(where(card({ outcome: 'deferred', approval: 'Proceed?' }))).toBe('inbox');
  });

  test('shells are not conversations', () => {
    expect(where(card({ phase: 'shell', agent: null }))).toBe('none');
  });
});
