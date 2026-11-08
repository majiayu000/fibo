// The 待办 tab: every todo you have not closed, by space, and the conversations nobody has
// touched for days. What runs now is the home screen's; this is what you wrote down to do.
// Pure, so tests run it without React Native.
import type { Card, Machine, Todo } from '../remote/hub';
import { spaceName } from './status';

export type TodoItem = { m: Machine; todo: Todo };

export const todoKey = ({ m, todo }: TodoItem) => `${m.id}/${todo.id}`;

/** The space a todo is listed under, by the name you see, as conversations are. */
export const todoSpace = (m: Machine, t: Todo) => m.spaces[t.space] || t.space;

/** When it last moved: its newest progress, else when you noted it. */
export const touched = (t: Todo) => t.notes?.at(-1)?.at ?? t.created ?? 0;

/** Waiting on you to judge first, then what is being worked on, then what nobody started. */
const ORDER: Record<string, number> = { proven: 0, settled: 1, working: 2, not_started: 3, deferred: 4 };
const order = (t: Todo) => ORDER[t.state ?? ''] ?? 3;

/** How a state's words are coloured: yours to judge, being worked on, or quiet. */
export function todoTone(t: Todo): 'ok' | 'run' | 'dim' {
  if (t.state === 'proven' || t.state === 'settled') return 'ok';
  if (t.state === 'working') return 'run';
  return 'dim';
}

export type TodoChip = { space: string | null; count: number };

export type TodoShelf = {
  /** The space shown, null for all; a saved one that has gone reads as all. */
  filter: string | null;
  /** Empty with fewer than two spaces. By name, so a chip never moves under a finger. */
  chips: TodoChip[];
  /** By space name, each newest-moved first within its order. */
  groups: { space: string; items: TodoItem[] }[];
  total: number;
  /** Conversations untouched past the computer's stale days, in the space shown. */
  stale: { m: Machine; c: Card }[];
};

export function shelveTodos(machines: Machine[], selected: string | null): TodoShelf {
  const all: TodoItem[] = machines.flatMap((m) => (m.todos ?? []).map((todo) => ({ m, todo })));
  const name = (i: TodoItem) => todoSpace(i.m, i.todo);
  const names = [...new Set(all.map(name))].sort((a, b) => a.localeCompare(b));
  const chips = names.length < 2 ? [] : [{ space: null, count: all.length }, ...names.map((n) => ({ space: n, count: all.filter((i) => name(i) === n).length }))];
  const filter = selected !== null && names.includes(selected) ? selected : null;
  const shown = all
    .filter((i) => filter === null || name(i) === filter)
    .sort((a, b) => order(a.todo) - order(b.todo) || touched(b.todo) - touched(a.todo));
  const groups = new Map<string, TodoItem[]>();
  for (const i of shown) groups.set(name(i), [...(groups.get(name(i)) ?? []), i]);
  const stale = machines
    .flatMap((m) => m.cards.filter((c) => c.column === 'stale' && !!c.agent).map((c) => ({ m, c })))
    .filter((i) => filter === null || spaceName(i.m, i.c) === filter)
    .sort((a, b) => (b.c.updated ?? 0) - (a.c.updated ?? 0));
  return { filter, chips, groups: [...groups].map(([n, items]) => ({ space: n, items })), total: all.length, stale };
}

/** The line under a todo's title: its newest progress, one line of it. */
export const lastNote = (t: Todo) => t.notes?.at(-1)?.text.split('\n')[0] ?? '';
