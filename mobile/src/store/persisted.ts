/** The existing saved-value store, independent of the native Keychain adapter. */
import { notUi } from '../i18n';
type Storage = { get(key: string): Promise<string | null>; set(key: string, text: string): Promise<unknown> };

export function persistedValue<T extends object>(storage: Storage, key: string, initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  const emit = () => listeners.forEach((listener) => listener());
  async function read() {
    const text = await storage.get(key);
    if (text === null) return initial;
    const parsed = JSON.parse(text);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error(notUi('保存的数据无法读取'));
    // A value an older build saved lacks the fields added since: they read as their defaults.
    return { ...initial, ...parsed } as T;
  }
  let ready = read().then((next) => { value = next; emit(); });
  // Consumers may mount later. Keep the rejection available to them without an
  // unhandled promise during module initialization.
  void ready.catch(() => undefined);
  let pending: Promise<unknown> = Promise.resolve();
  return {
    get ready() { return ready; },
    get: () => value,
    reload() {
      ready = pending.then(read).then((next) => { value = next; emit(); });
      void ready.catch(() => undefined);
      return ready;
    },
    set(patch: Partial<T> | ((v: T) => T)): Promise<void> {
      const loaded = ready;
      const changed = pending.then(async () => {
        await loaded;
        const next = typeof patch === 'function' ? patch(value) : { ...value, ...patch };
        await storage.set(key, JSON.stringify(next));
        value = next;
        emit();
      });
      // Serialize read-modify-write operations, including a retry after a failed write.
      pending = changed.catch(() => undefined);
      return changed;
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
  };
}
