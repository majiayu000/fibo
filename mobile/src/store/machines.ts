// Paired machines, each its pairing link. The link holds this device's key, so it lives
// in the Keychain / Keystore (expo-secure-store), one entry per machine to stay under the
// ~2 KB some iOS versions accept per value. On web, used only to try the UI in a browser,
// it falls back to localStorage.
import { type Link, linkText, parseLink } from '../remote/link';
import { kv as store } from './kv';
import { tr } from '../i18n';

const INDEX = 'mooring.machines';
const entry = (id: string) => `mooring.machine.${id.replace(/[^A-Za-z0-9._-]/g, '_')}`;

/** A machine is known by its id; a link without one (older machines) by its device. */
export const machineId = (l: Link) => l.machine || `device-${l.device}`;

async function ids(): Promise<string[]> {
  const value: unknown = JSON.parse((await store.get(INDEX)) ?? '[]');
  if (!Array.isArray(value) || !value.every((id) => typeof id === 'string')) throw new Error(tr('本地数据格式不正确'));
  return value;
}

export async function loadMachines(): Promise<Link[]> {
  const out: Link[] = [];
  for (const id of await ids()) {
    const text = await store.get(entry(id));
    if (!text) continue;
    try {
      out.push(parseLink(text));
    } catch {
      // A link this version cannot read is skipped, not deleted.
    }
  }
  return out;
}

/** Pairing the same machine again replaces its old link. */
export async function saveMachine(link: Link): Promise<void> {
  const id = machineId(link);
  const all = await ids();
  await store.set(entry(id), linkText(link));
  if (!all.includes(id)) await store.set(INDEX, JSON.stringify([...all, id]));
}

export async function forgetMachine(id: string): Promise<void> {
  const all = await ids();
  await store.del(entry(id));
  await store.set(INDEX, JSON.stringify(all.filter((x) => x !== id)));
}
