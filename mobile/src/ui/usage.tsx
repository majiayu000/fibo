// Today's tokens on one computer, from its Agents' own logs (`usage`). No money: the
// computer has no price it could vouch for. The first read of a big log takes the computer
// a few asks; numbers grow until it says it is done.
import { useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { tr } from '../i18n';
import { type Machine, remote, supports } from '../remote/hub';
import { compact } from './parse';
import { agentName } from './status';
import { useTheme } from './theme';

type Reading = { agents: { agent: string; tokens: number; cached: number }[]; partial: boolean };

const AGAIN_MS = 1500;
const EVERY_MS = 60_000;

/** The phone's own midnight: 今天 is the day of whoever is holding it. */
const midnight = () => Math.floor(new Date().setHours(0, 0, 0, 0) / 1000);

export function TodayTokens({ m }: { m: Machine }) {
  const t = useTheme();
  const [got, setGot] = useState<Reading | null>(null);
  const able = supports(m, 'usage');

  useEffect(() => {
    const r = able ? remote(m.id) : null;
    if (!r) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const load = () =>
      r
        .request<Reading>('usage', { since: midnight() })
        .then((u) => {
          if (!live) return;
          setGot(u);
          timer = setTimeout(load, u.partial ? AGAIN_MS : EVERY_MS);
        })
        // The tile stays out of the way; the page says what is wrong with the link.
        .catch(() => undefined);
    load();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [able, m.id]);

  if (!got) return null;
  const tokens = got.agents.reduce((n, a) => n + a.tokens, 0);
  const cached = got.agents.reduce((n, a) => n + a.cached, 0);
  return (
    <View style={[s.card, { backgroundColor: t.card, borderColor: t.line }]}>
      <View style={s.row}>
        <Text style={{ fontSize: 12, color: t.dim }}>{tr('今天 tokens')}</Text>
        {got.partial && <Text style={{ fontSize: 12, color: t.dim }}>{tr('还在读日志…')}</Text>}
      </View>
      <Text style={{ fontSize: 22, fontWeight: '700', color: t.text }}>{compact(tokens)}</Text>
      <Text numberOfLines={2} style={{ fontSize: 12, lineHeight: 17, color: t.dim }}>
        {[...got.agents.map((a) => `${agentName(a.agent)} ${compact(a.tokens)}`), cached ? tr('另有缓存读取 {n}', { n: compact(cached) }) : ''].filter(Boolean).join(' · ') || tr('今天还没有用量')}
      </Text>
    </View>
  );
}

const s = StyleSheet.create({
  card: { borderWidth: 1, borderRadius: 16, padding: 14, gap: 4 },
  row: { flexDirection: 'row', justifyContent: 'space-between' },
});
