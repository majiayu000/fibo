// Quota at a glance: the fullest window of each Agent, on each computer's card under 我的. The
// home screen shows none: a line there read as part of the list it sat over (2026-11-09).
// As on 额度, every reading comes from the paired computer itself; the phone asks nobody else.
import { useFocusEffect } from 'expo-router';
import { useCallback, useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { tr } from '@/i18n';
import { type Machine, remote, supports } from '@/remote/hub';
import { type QuotaLevel, quotaStore } from '@/store/quota';
import { type Tight, shortWindow, tightWindows } from './parse';
import { useNow } from './now';
import { agentName } from './status';
import { type Theme } from './theme';

/** A glance can be minutes old; 额度 itself reads every minute while it is open. */
const GLANCE_MS = 5 * 60_000;

/** The online computers' fullest windows, fullest first, kept fresh while this screen shows. */
export function useTightWindows(machines: Machine[]): Tight[] {
  const able = machines.filter((m) => m.status === 'online' && supports(m, 'quota'));
  const ids = able.map((m) => m.id).join('\n');
  const read = useCallback(
    () => tightWindows(ids ? ids.split('\n').map((id) => ({ machine: machines.find((m) => m.id === id)?.name || tr('电脑'), agents: quotaStore(id).get().agents })) : []),
    // `ids` says which computers; their names do not change what is read.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ids],
  );
  const [tight, setTight] = useState<Tight[]>(read);
  const now = useNow();
  useEffect(() => setTight(read()), [now, read]);
  useEffect(() => {
    setTight(read());
    const subs = ids ? ids.split('\n').map((id) => quotaStore(id).subscribe(() => setTight(read()))) : [];
    return () => subs.forEach((off) => off());
  }, [ids, read]);
  useFocusEffect(
    useCallback(() => {
      if (!ids) return;
      const load = () => {
        for (const id of ids.split('\n')) {
          remote(id)
            ?.request('quota', {})
            .then((q) => quotaStore(id).set({ agents: q.agents ?? [] }))
            // The line keeps what the computer last said; 额度 is where a failure is told.
            .catch(() => undefined);
        }
      };
      load();
      const timer = setInterval(load, GLANCE_MS);
      return () => clearInterval(timer);
    }, [ids]),
  );
  return tight;
}

export function tightWords(q: Tight, named: boolean) {
  return `${named ? `${q.machine} · ` : ''}${agentName(q)} ${shortWindow(q.minutes)} ${q.used}%`;
}

export function Bar({ used, level, t, width }: { used: number; level?: QuotaLevel; t: Theme; width?: number }) {
  return (
    <View style={[s.track, { backgroundColor: t.empty }, width ? { width } : { alignSelf: 'stretch' }]}>
      <View style={[s.fill, { width: `${Math.min(used, 100)}%`, backgroundColor: level ? t[level] : t.dim }]} />
    </View>
  );
}

const s = StyleSheet.create({
  track: { height: 5, borderRadius: 3, overflow: 'hidden' },
  fill: { height: 5, borderRadius: 3 },
});
