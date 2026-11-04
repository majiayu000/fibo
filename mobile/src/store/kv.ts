// Small values kept on this phone: the Keychain / Keystore (expo-secure-store) on a phone,
// localStorage on web, which serves only to try the UI in a browser.
import * as SecureStore from 'expo-secure-store';
import { Platform } from 'react-native';

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

/** A JSON value under `key`, `fallback` when absent or unreadable. */
export async function load<T>(key: string, fallback: T): Promise<T> {
  try {
    const text = await kv.get(key);
    return text ? { ...fallback, ...JSON.parse(text) } : fallback;
  } catch {
    return fallback;
  }
}

export const save = (key: string, value: unknown) => kv.set(key, JSON.stringify(value)).catch(() => undefined);

/** A value screens read with a hook and change with `set`; saved under `key` on each change. */
export function persisted<T extends object>(key: string, initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  const ready = load(key, initial).then((v) => {
    value = v;
    listeners.forEach((l) => l());
  });
  return {
    ready,
    get: () => value,
    set(patch: Partial<T> | ((v: T) => T)) {
      value = typeof patch === 'function' ? patch(value) : { ...value, ...patch };
      listeners.forEach((l) => l());
      save(key, value);
    },
    subscribe(l: () => void) {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
  };
}
