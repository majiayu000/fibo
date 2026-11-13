// Small values kept on this phone: the Keychain / Keystore (expo-secure-store) on a phone,
// localStorage on web, which serves only to try the UI in a browser.
import * as SecureStore from 'expo-secure-store';
import { Platform } from 'react-native';
import { persistedValue } from './persisted';

export const kv =
  Platform.OS === 'web'
    ? {
        get: async (k: string) => globalThis.localStorage?.getItem(k) ?? null,
        set: async (k: string, v: string) => globalThis.localStorage?.setItem(k, v),
        del: async (k: string) => globalThis.localStorage?.removeItem(k),
      }
    : {
        get: (k: string) => SecureStore.getItemAsync(k),
        set: (k: string, v: string) => SecureStore.setItemAsync(k, v),
        del: (k: string) => SecureStore.deleteItemAsync(k),
      };

/** A value screens read with a hook and change with `set`; saved under `key` on each change. */
export function persisted<T extends object>(key: string, initial: T) {
  return persistedValue(kv, key, initial);
}

/**
 * A preference whose loss costs nothing (how the home screen was folded, a count): it changes at
 * once, a failed save is dropped, and an unreadable one starts from `initial`. What must not be
 * lost or read wrong (settings, the outbox, what was reviewed) uses `persisted`.
 */
export function remembered<T extends object>(key: string, initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  const emit = () => listeners.forEach((l) => l());
  const ready = kv
    .get(key)
    .then((text) => {
      if (text) value = { ...initial, ...JSON.parse(text) };
    })
    .catch(() => undefined)
    .then(emit);
  return {
    ready,
    get: () => value,
    set(patch: Partial<T> | ((v: T) => T)) {
      value = typeof patch === 'function' ? patch(value) : { ...value, ...patch };
      emit();
      kv.set(key, JSON.stringify(value)).catch(() => undefined);
    },
    subscribe(l: () => void) {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
  };
}
