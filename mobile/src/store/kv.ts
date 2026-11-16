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
export function persisted<T extends object>(key: string, initial: T, options?: { replaceable?: boolean }) {
  return persistedValue(kv, key, initial, options);
}

/** Optional preferences share the same safe store; their caller need not handle save errors. */
export function remembered<T extends object>(key: string, initial: T) {
  const store = persistedValue(kv, key, initial);
  return {
    ready: store.ready.catch(() => undefined),
    get: store.get,
    subscribe: store.subscribe,
    set(patch: Partial<T> | ((v: T) => T)) {
      void store.set(patch).catch(() => undefined);
    },
  };
}
