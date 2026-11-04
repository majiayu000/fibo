// What this phone remembers about how it should behave; none of it reaches the computer.
// Notifications are switches only: push is not wired yet (docs/phone-app/v2-plan.md).
import { useSyncExternalStore } from 'react';
import { persisted } from './kv';

export type Settings = {
  appearance: 'system' | 'light' | 'dark';
  /** Face ID / fingerprint on opening the app and before every 允许. */
  lock: boolean;
  /** Ask once more before allowing a push, rm, deploy and the like. */
  confirmDanger: boolean;
  notify: { approval: boolean; waiting: boolean; error: boolean; done: boolean; quiet: boolean };
};

export const settings = persisted<Settings>('mooring.settings', {
  appearance: 'system',
  lock: false,
  confirmDanger: true,
  notify: { approval: true, waiting: true, error: true, done: false, quiet: false },
});

export const useSettings = () => useSyncExternalStore(settings.subscribe, settings.get);
