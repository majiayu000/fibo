// Sends what waits in the outbox as soon as its computer is online again; nothing to draw.
import { useEffect, useRef } from 'react';
import { drop, mark, useOutbox } from '../store/outbox';
import { useHub, write } from './hub';

export function Pump() {
  const { loaded, machines } = useHub();
  const items = useOutbox();
  const flying = useRef(new Set<string>());
  useEffect(() => {
    if (!loaded) return;
    for (const u of items) {
      if (flying.current.has(u.id)) continue;
      const m = machines.find((x) => x.id === u.machine);
      // Its computer was removed from this phone: nowhere for it to go.
      if (!m) {
        drop(u.id);
        continue;
      }
      if (u.error) continue;
      if (m?.status !== 'online' || !m.ops.includes('send')) continue;
      flying.current.add(u.id);
      write(u.machine, 'send', { terminal: u.terminal, text: u.text, enter: true, request: u.id })
        .then(() => drop(u.id))
        .catch((e) => mark(u.id, (e as Error).message))
        .finally(() => flying.current.delete(u.id));
    }
  }, [items, loaded, machines]);
  return null;
}
