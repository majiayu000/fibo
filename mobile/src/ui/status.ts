// How a conversation's state reads and what colour it takes. The host decides the state
// (`status::of` on the desktop, `status` and `status_label` on each card) and spells it;
// a phone only pairs with hosts new enough to send both.
import type { Card, Machine } from '../remote/hub';
import { hostWords, tr } from '../i18n';
import type { Theme } from './theme';

export function statusKey(c: Card): string {
  if (c.approval) return 'approval';
  return c.status ?? 'shell';
}

export function statusWord(c: Card): string {
  if (c.approval) return tr('等你审批');
  if (c.status_label) return hostWords(c.status_label);
  return c.phase === 'shell' ? tr('终端') : '';
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

/** What a row's third line says after its branch: what it is doing now, else its Agent. */
export const rowDetail = (c: Card) => c.doing || agentName(c.agent);

export const agentName = (agent?: string | null) =>
  agent === 'claude' ? 'Claude Code' : agent === 'codex' ? 'Codex' : agent ? agent[0].toUpperCase() + agent.slice(1) : '';

/** A path with the home directory as `~`. */
export function tilde(path: string): string {
  return path.replace(/^\/(Users|home)\/[^/]+(?=\/|$)/, '~');
}

/** The last directory of a path. */
export const base = (path: string) => path.split('/').filter(Boolean).pop() ?? path;

/** The space a conversation is listed under, by the name you see: one project on two computers is one space. */
export const spaceName = (m: Machine, c: Card) => m.spaces[c.space] || base(c.cwd) || c.space;
