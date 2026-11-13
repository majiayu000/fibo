// Sends what waits in the outbox as soon as its computer is online again; nothing to draw.
import { useEffect, useRef } from 'react';
import { drop, mark, useOutbox } from '../store/outbox';
import { useHub, write } from './hub';
import { oops } from '../actions';
import { tr } from '../i18n';

export function Pump() {
  const { loaded, machines } = useHub();
  const items = useOutbox();
  const flying = useRef(new Set<string>());
  const storageFailed = useRef(new Set<string>());
  useEffect(() => {
    if (!loaded) return;
    for (const u of items) {
      if (flying.current.has(u.id) || storageFailed.current.has(u.id)) continue;
      const failed = () => {
        storageFailed.current.add(u.id);
        oops(tr('未能保存待发送消息的状态，已暂停自动重试。请重启 Fibo 后再试；原消息编号会保留，避免重复执行。'));
      };
      const m = machines.find((x) => x.id === u.machine);
      // Its computer was removed from this phone: nowhere for it to go.
      if (!m) {
        flying.current.add(u.id);
        drop(u.id).catch(failed).finally(() => flying.current.delete(u.id));
        continue;
      }
      if (u.error) continue;
      if (m?.status !== 'online' || !m.ops.includes('send')) continue;
      flying.current.add(u.id);
      write(u.machine, 'send', { terminal: u.terminal, session: u.session ?? '', text: u.text, enter: true, request: u.id })
        .then(() => drop(u.id), (e) => mark(u.id, (e as Error).message))
        .catch(failed)
        .finally(() => flying.current.delete(u.id));
    }
  }, [items, loaded, machines]);
  return null;
}
