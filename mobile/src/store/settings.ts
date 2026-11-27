// What this phone remembers about how it should behave; none of it reaches the computer.
// Notifications are switches only: push is not wired yet (docs/phone-app/v2-plan.md).
import { useSyncExternalStore } from 'react';
import { notUi } from '../i18n';
import { persisted } from './kv';

export type Settings = {
  appearance: 'system' | 'light' | 'dark';
  /** 自动 follows the phone's language; a phone saved before this had no field and reads as auto. */
  language: 'auto' | 'zh' | 'en';
  /** The lock's check (ui/lockway.ts) on opening the app and before every 允许. */
  lock: boolean;
  /** How strict that check is on Android; none: strict. */
  lockWay?: 'strong' | 'weak';
  /** Ask once more before allowing a push, rm, deploy and the like. */
  confirmDanger: boolean;
  notify: { approval: boolean; waiting: boolean; error: boolean; done: boolean; quiet: boolean };
};

export const settings = persisted<Settings>('mooring.settings', {
  appearance: 'system',
  language: 'auto',
  lock: false,
  confirmDanger: true,
  notify: { approval: true, waiting: true, error: true, done: false, quiet: false },
});

export const useSettings = () => useSyncExternalStore(settings.subscribe, settings.get);

/** An unreadable security choice must never become an unlocked app. */
export async function readSettings() {
  await settings.ready;
  const value = settings.get();
  if (typeof value.lock !== 'boolean' || typeof value.confirmDanger !== 'boolean') throw new Error(notUi('安全设置无法读取'));
  if (value.lockWay !== undefined && value.lockWay !== 'strong' && value.lockWay !== 'weak') throw new Error(notUi('安全设置无法读取'));
  return value;
}
