// The lock's look on this phone: its words and icon, from what the phone has (ui/lockway.ts).
import { useSyncExternalStore } from 'react';
import { lockKit } from '../actions';
import { tr } from '../i18n';
import { useSettings } from '../store/settings';
import { inline, type LockKit, type LockWay, lockChoices, lockWay, wayIcon, wayWords } from './lockway';

/** `inline`: the words to put inside a sentence (lockway.ts `inline`). */
export type LockLook = { kit: LockKit | null; way: LockWay; words: string; inline: string; icon: 'face' | 'lock'; choices: LockWay[] };

// The layout refreshes it on launch, settings before turning the lock on. All lock labels use that result.
let known: LockKit | null = null;
let asking: Promise<LockKit> | null = null;
const listeners = new Set<() => void>();
const getKit = () => known;

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

export function refreshLockKit(): Promise<LockKit> {
  asking ??= lockKit().then((kit) => {
    known = kit;
    for (const listener of listeners) listener();
    return kit;
  }).finally(() => { asking = null; });
  return asking;
}

export function useLockLook(): LockLook {
  const v = useSettings();
  const kit = useSyncExternalStore(subscribe, getKit, getKit);
  // Until the phone has said what it has: the plain words, never a face it may not have.
  if (!kit) return { kit, way: 'strong', words: tr('身份验证'), inline: tr('身份验证'), icon: 'lock', choices: [] };
  const way = lockWay(v.lockWay, kit);
  const words = wayWords(way, kit);
  return { kit, way, words, inline: inline(words), icon: wayIcon(way, kit), choices: lockChoices(kit) };
}

/** The icon on 允许: the lock's, while 允许 asks for it; none when it does not. */
export function useAllowIcon(): 'face' | 'lock' | undefined {
  const v = useSettings();
  const look = useLockLook();
  return v.lock ? look.icon : undefined;
}
