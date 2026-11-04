// How much of each Agent's limits is used, as the computer last read it: one card per
// Agent, a striped gauge per window, orange past 80%.
import { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { NOT_YET, remote, supports, useHub } from '@/remote/hub';
import { Nav, Notice, useBottom } from '@/ui/kit';
import { level, resetText, windowName } from '@/ui/parse';
import { agentName } from '@/ui/status';
import { type Theme, ago, useTheme } from '@/ui/theme';

type Window = { minutes: number; used: number; resets_at: number };
type Reading = { agent: string; plan?: string; at: number; windows: Window[] };

const STRIPES = 20;

function Gauge({ used, t }: { used: number; t: Theme }) {
  const lit = Math.round((Math.min(used, 100) / 100) * STRIPES);
  const color = t[level(used)];
  return (
    <View style={s.gauge} accessibilityLabel={`用了 ${used}%`}>
      {Array.from({ length: STRIPES }, (_, i) => (
        <View key={i} style={[s.stripe, { backgroundColor: i < lit ? color : t.empty }]} />
      ))}
    </View>
  );
}

function AgentCard({ r, t }: { r: Reading; t: Theme }) {
  return (
    <View style={[s.card, { backgroundColor: t.card, borderColor: t.line }]}>
      <View style={s.row}>
        <Text style={{ fontSize: 17, fontWeight: '600', color: t.text }}>{agentName(r.agent)}</Text>
        {!!r.plan && <Text style={{ fontSize: 13, color: t.dim }}>{r.plan[0].toUpperCase() + r.plan.slice(1)} 计划</Text>}
      </View>
      {r.windows.length === 0 && <Text style={{ color: t.dim, fontSize: 13 }}>没有读数</Text>}
      {r.windows.map((w) => {
        const reset = resetText(w.resets_at);
        return (
          <View key={w.minutes} style={{ gap: 8 }}>
            <View style={s.row}>
              <Text style={{ fontSize: 15, color: t.text }}>{windowName(w.minutes)}</Text>
              <Text style={{ fontSize: 14, color: t.dim }}>
                <Text style={{ fontWeight: '700', color: level(w.used) === 'high' ? t.high : t.text }}>{w.used}%</Text>
                {reset ? ` · ${reset}` : ''}
              </Text>
            </View>
            <Gauge used={w.used} t={t} />
          </View>
        );
      })}
    </View>
  );
}

export default function Quota() {
  const t = useTheme();
  const bottom = useBottom();
  const { machines } = useHub();
  const able = machines.filter((m) => supports(m, 'quota'));
  const [pick, setPick] = useState<string | null>(null);
  const m = able.find((x) => x.id === pick) ?? able[0];
  const [data, setData] = useState<{ machine: string; agents: Reading[] } | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    const r = m ? remote(m.id) : null;
    if (!r || !m) return;
    let live = true;
    const load = () =>
      r
        .request('quota', {})
        .then((q) => {
          if (!live) return;
          setData({ machine: m.id, agents: q.agents ?? [] });
          setError('');
        })
        .catch((e) => live && setError((e as Error).message));
    load();
    const timer = setInterval(load, 60_000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [m?.id]);

  const agents = data?.machine === m?.id ? data?.agents : undefined;
  const newest = agents?.reduce((n, a) => Math.max(n, a.at), 0);
  const online = machines.some((x) => x.status === 'online');

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav big title="额度" sub={m ? `来自 ${m.name}${newest ? ` · ${ago(newest)}更新` : ''}` : undefined} />
      {able.length > 1 && (
        <View style={{ flexDirection: 'row', gap: 8, paddingHorizontal: 18, paddingBottom: 6 }}>
          {able.map((x) => (
            <Pressable key={x.id} onPress={() => setPick(x.id)} style={[s.chip, { borderColor: x.id === m?.id ? t.text : t.line }]}>
              <Text style={{ fontSize: 13, color: t.text }}>{x.name}</Text>
            </Pressable>
          ))}
        </View>
      )}
      {!online ? (
        <Notice text="电脑没连上" />
      ) : !m ? (
        <Notice text={NOT_YET} />
      ) : error ? (
        <Notice text={error} />
      ) : !agents ? (
        <ActivityIndicator style={{ marginTop: 40 }} />
      ) : (
        <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 14, paddingBottom: bottom + 16 }}>
          {agents.length === 0 && <Notice text="这一周电脑上的 Agent 都没报过额度" />}
          {agents.map((a) => (
            <AgentCard key={a.agent} r={a} t={t} />
          ))}
        </ScrollView>
      )}
    </View>
  );
}

const s = StyleSheet.create({
  card: { borderWidth: 1, borderRadius: 16, padding: 14, gap: 14 },
  row: { flexDirection: 'row', alignItems: 'baseline', justifyContent: 'space-between', gap: 8 },
  gauge: { flexDirection: 'row', gap: 2, height: 10 },
  stripe: { flex: 1, borderRadius: 1 },
  chip: { height: 32, borderRadius: 16, borderWidth: 1, paddingHorizontal: 12, justifyContent: 'center' },
});
