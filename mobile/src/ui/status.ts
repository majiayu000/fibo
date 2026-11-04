// How a conversation's state reads and what colour it takes. The host decides the state
// (`status::of` on the desktop, `status` and `status_label` on each card); the fallbacks
// only serve hosts from before 11-03.
import type { Card } from '../remote/hub';
import type { Theme } from './theme';

export const needsYou = (c: Card) => c.needs_you ?? Boolean(c.approval || c.phase === 'waiting');

export function statusKey(c: Card): string {
  if (c.approval) return 'approval';
  if (c.status) return c.status;
  if (c.phase === 'waiting') return 'waiting';
  if (c.phase === 'working') return 'running';
  return c.phase === 'shell' ? 'shell' : 'review';
}

export function statusWord(c: Card): string {
  if (c.approval) return '等你审批';
  if (c.status_label) return c.status_label;
  switch (statusKey(c)) {
    case 'waiting':
      return '等你回答';
    case 'running':
      return '在跑';
    case 'shell':
      return '终端';
    default:
      return '等你验收';
  }
}

export function statusColor(c: Card, t: Theme): string {
  switch (statusKey(c)) {
    case 'approval':
    case 'waiting':
      return t.wait;
    case 'error':
      return t.err;
    case 'running':
    case 'background':
      return t.run;
    case 'review':
      return t.ok;
    default:
      return t.dim;
  }
}

/** Inbox order: what needs you, then errors, what runs, the rest; newest first within. */
export function rank(c: Card): number {
  const k = statusKey(c);
  if (k === 'approval') return 0;
  if (k === 'waiting') return 1;
  if (k === 'error') return 2;
  if (k === 'running' || k === 'background') return 3;
  if (k === 'review') return 4;
  return 5;
}

export const agentName = (agent?: string | null) =>
  agent === 'claude' ? 'Claude Code' : agent === 'codex' ? 'Codex' : agent ? agent[0].toUpperCase() + agent.slice(1) : '';

/** A path with the home directory as `~`. */
export function tilde(path: string): string {
  return path.replace(/^\/(Users|home)\/[^/]+(?=\/|$)/, '~');
}

/** The last directory of a path. */
export const base = (path: string) => path.split('/').filter(Boolean).pop() ?? path;
