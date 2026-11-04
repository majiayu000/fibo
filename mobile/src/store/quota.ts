// The last reading each computer gave, kept on this phone so a computer that went offline
// still shows what it said, marked as such. It only ever comes from that computer; the
// phone asks nobody else. One entry per computer, to stay under the ~2 KB limit per value.
import { kv, persisted } from './kv';

export type QuotaWindow = { minutes: number; used: number; resets_at: number };
export type QuotaReading = { agent: string; plan?: string; at: number; windows: QuotaWindow[] };
export type QuotaSaved = { agents: QuotaReading[] | null };

const entry = (id: string) => `mooring.quota.${id.replace(/[^A-Za-z0-9._-]/g, '_')}`;
const stores = new Map<string, ReturnType<typeof persisted<QuotaSaved>>>();

export function quotaStore(machine: string) {
  let s = stores.get(machine);
  if (!s) {
    s = persisted<QuotaSaved>(entry(machine), { agents: null });
    stores.set(machine, s);
  }
  return s;
}

/** Unpairing a computer drops what it said. */
export function forgetQuota(machine: string) {
  stores.delete(machine);
  return kv.del(entry(machine)).catch(() => undefined);
}
