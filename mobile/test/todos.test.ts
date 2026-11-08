// The 待办 tab reads todos as the computer sends them, old computers' four fields included.
import { describe, expect, test } from 'bun:test';
import type { Card, Machine, Todo } from '../src/remote/hub';
import { lastNote, shelveTodos, todoTone, touched } from '../src/ui/todos';

const todo = (id: string, space: string, more: Partial<Todo> = {}): Todo => ({ id, space, title: id, agent: 'claude', ...more });
const machine = (todos: Todo[], cards: Partial<Card>[] = [], id = 'm') =>
  ({ id, status: 'online', cards: cards as Card[], spaces: { a: 'alpha', b: 'beta' }, todos }) as unknown as Machine;
const ids = (s: ReturnType<typeof shelveTodos>) => s.groups.map((g) => [g.space, g.items.map((i) => i.todo.id)]);

describe('the 待办 tab', () => {
  test('groups by space name, yours to judge first, then worked on, then not started, newest moved first', () => {
    const s = shelveTodos(
      [
        machine([
          todo('idle-old', 'a', { state: 'not_started', created: 1 }),
          todo('idle-new', 'a', { state: 'not_started', created: 5 }),
          todo('working', 'a', { state: 'working', created: 2 }),
          todo('judge', 'a', { state: 'settled', created: 0 }),
          todo('other', 'b', { state: 'working' }),
        ]),
      ],
      null,
    );
    expect(ids(s)).toEqual([
      ['alpha', ['judge', 'working', 'idle-new', 'idle-old']],
      ['beta', ['other']],
    ]);
    expect(s.total).toBe(5);
    expect(s.chips.map((c) => [c.space, c.count])).toEqual([
      [null, 5],
      ['alpha', 4],
      ['beta', 1],
    ]);
  });
  test('a chip shows one space; a space that has gone reads as all; one space has no chips', () => {
    const m = machine([todo('x', 'a'), todo('y', 'b')]);
    expect(ids(shelveTodos([m], 'beta'))).toEqual([['beta', ['y']]]);
    expect(shelveTodos([m], 'gone').filter).toBeNull();
    expect(shelveTodos([machine([todo('x', 'a')])], null).chips).toEqual([]);
  });
  test('an old computer sends four fields: listed as not started, nothing more', () => {
    const t = todo('old', 'a');
    expect(todoTone(t)).toBe('dim');
    expect(lastNote(t)).toBe('');
    expect(touched(t)).toBe(0);
    expect(ids(shelveTodos([machine([t])], null))).toEqual([['alpha', ['old']]]);
  });
  test('progress: the newest note, its first line, moves it', () => {
    const t = todo('n', 'a', { created: 1, notes: [{ at: 3, text: 'first' }, { at: 9, text: 'PR open\nwaiting on CI' }] });
    expect(lastNote(t)).toBe('PR open');
    expect(touched(t)).toBe(9);
  });
  test('stale conversations come from the computer’s column, in the space shown', () => {
    const m = machine([], [
      { id: 'old', agent: 'claude', column: 'stale', space: 'a', cwd: '/x', updated: 1 },
      { id: 'run', agent: 'claude', column: 'running', space: 'a', cwd: '/x' },
      { id: 'shell', agent: null, column: 'stale', space: 'a', cwd: '/x' },
      { id: 'b-old', agent: 'codex', column: 'stale', space: 'b', cwd: '/y', updated: 2 },
    ]);
    expect(shelveTodos([m], null).stale.map((i) => i.c.id)).toEqual(['b-old', 'old']);
    expect(shelveTodos([m, machine([todo('t', 'a')], [], 'n')], 'alpha').stale.map((i) => i.c.id)).toEqual(['old']);
  });
});
