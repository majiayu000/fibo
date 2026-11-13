// Files kept in this phone's own documents folder: what is too big for the keychain (`kv`),
// such as the last messages of a conversation or copies of the pictures you sent. A
// browser (the web build, only for trying the UI) keeps none.
import { Directory, File, Paths } from 'expo-file-system';
import { Platform } from 'react-native';

const kept = Platform.OS !== 'web';

/** The folder `name` under documents, made when missing; null where files cannot be kept. */
export function folder(name: string): Directory | null {
  if (!kept) return null;
  try {
    const dir = new Directory(Paths.document, name);
    if (!dir.exists) dir.create({ intermediates: true, idempotent: true });
    return dir;
  } catch {
    return null;
  }
}

/** A JSON file's value, `fallback` when it is missing or unreadable. */
export function readJson<T>(dir: string, name: string, fallback: T): T {
  const d = folder(dir);
  if (!d) return fallback;
  try {
    const f = new File(d, name);
    return f.exists ? (JSON.parse(f.textSync()) as T) : fallback;
  } catch {
    return fallback;
  }
}

export function writeJson(dir: string, name: string, value: unknown) {
  const d = folder(dir);
  if (!d) return;
  try {
    const f = new File(d, name);
    if (!f.exists) f.create({ overwrite: true });
    f.writeSync(JSON.stringify(value));
  } catch {
    // A full disk loses a cache, nothing more.
  }
}

/** Copies a file (a picture the picker left in the cache) into `dir`; its new uri, or null. */
export function keepCopy(dir: string, from: string, name: string): string | null {
  const d = folder(dir);
  if (!d) return null;
  try {
    const to = new File(d, name);
    if (to.exists) to.delete();
    new File(from).copySync(to);
    return to.uri;
  } catch {
    return null;
  }
}

export function remove(uri: string) {
  if (!kept) return;
  try {
    const f = new File(uri);
    if (f.exists) f.delete();
  } catch {
    // Already gone.
  }
}

/** Leaves the newest `most` files in `dir`, by when each was last written. */
export function keepNewestFiles(dir: string, most: number) {
  const d = folder(dir);
  if (!d) return;
  try {
    const files = d.list().filter((x): x is File => x instanceof File);
    if (files.length <= most) return;
    files.sort((a, b) => (b.modificationTime ?? 0) - (a.modificationTime ?? 0));
    for (const f of files.slice(most)) f.delete();
  } catch {
    // Kept as it is.
  }
}
