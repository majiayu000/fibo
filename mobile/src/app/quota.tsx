// How much of each Agent's limits is used, computer by computer and never added up: two
// computers may share an account or not, and only each one's own readings say anything.
// One card per Agent, a striped gauge per window, orange from 80%, red from 95%, as the host reports.
import { useEffect, useState, useSyncExternalStore } from 'react';
import { ActivityIndicator, ScrollView, StyleSheet, Text, View } from 'react-native';
import { type Machine, NOT_YET, remote, supports, useHub } from '@/remote/hub';
import { tr } from '@/i18n';
import { type QuotaReading as Reading, type QuotaLevel, quotaStore } from '@/store/quota';
import { Nav, Notice, useBottom } from '@/ui/kit';
import { quotaView, resetText, windowName } from '@/ui/parse';
import { agentName } from '@/ui/status';
import { type Theme, ago, useTheme } from '@/ui/theme';
import { TodayTokens } from '@/ui/usage';

const STRIPES = 20;

function Gauge({ used, level, t }: { used: number; level?: QuotaLevel; t: Theme }) {
  const lit = Math.round((Math.min(used, 100) / 100) * STRIPES);
  const color = level ? t[level] : t.dim;
  return (
    <View style={s.gauge} accessibilityLabel={tr('用了 {n}%', { n: used })}>
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
        <Text style={{ fontSize: 17, fontWeight: '600', color: t.text }}>{agentName(r)}</Text>
        {!!r.plan && <Text style={{ fontSize: 13, color: t.dim }}>{tr('{plan} 计划', { plan: r.plan[0].toUpperCase() + r.plan.slice(1) })}</Text>}
      </View>
      {r.windows.length === 0 && <Text style={{ color: t.dim, fontSize: 13 }}>{tr('没有读数')}</Text>}
      {r.windows.map((w) => {
        const reset = resetText(w.resets_at);
        return (
          <View key={w.minutes} style={{ gap: 8 }}>
            <View style={s.row}>
              <Text style={{ fontSize: 15, color: t.text }}>{windowName(w.minutes)}</Text>
              <Text style={{ fontSize: 14, color: t.dim }}>
                <Text style={{ fontWeight: '700', color: w.level === 'high' ? t.high : t.text }}>{w.used}%</Text>
                {reset ? ` · ${reset}` : ''}
              </Text>
            </View>
            <Gauge used={w.used} level={w.level} t={t} />
          </View>
        );
      })}
    </View>
  );
}

/** One computer's readings: live while it is on, what it last said once it is off. */
function MachineQuota({ m, named, t }: { m: Machine; named: boolean; t: Theme }) {
  const store = quotaStore(m.id);
  const saved = useSyncExternalStore(store.subscribe, store.get).agents;
  const [error, setError] = useState('');
  const able = supports(m, 'quota');

  useEffect(() => {
    const r = able ? remote(m.id) : null;
    if (!r) return;
    let live = true;
    const load = () =>
      r
        .request('quota', {})
        .then((q) => {
          if (!live) return;
          store.set({ agents: q.agents ?? [] });
          setError('');
        })
        .catch((e) => live && setError((e as Error).message));
    load();
    const timer = setInterval(load, 60_000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [able, m.id, store]);

  const view = quotaView(m.status === 'online', m.ops.includes('quota'), saved);
  const newest = saved?.reduce((n, a) => Math.max(n, a.at), 0);
  const when = newest ? ago(newest) : '';
  const line = view === 'gone' ? (when ? tr('断开前 · {when}的读数', { when }) : tr('断开前')) : when ? tr('{when}更新', { when }) : '';

  return (
    <View style={{ gap: 14 }}>
      {named && (
        <View style={s.row}>
          <Text style={{ fontSize: 15, fontWeight: '600', color: t.text }}>{m.name}</Text>
          {!!line && <Text style={{ fontSize: 13, color: t.dim }}>{line}</Text>}
        </View>
      )}
      {!named && view === 'gone' && <Text style={{ fontSize: 13, color: t.dim }}>{line}</Text>}
      {view === 'unsupported' && <Notice text={tr(NOT_YET)} />}
      {view === 'never' && <Notice text={tr('电脑没连上，也还没读到过')} />}
      {view !== 'never' && view !== 'unsupported' && !!error && <Notice text={error} />}
      {view === 'loading' && !error && <ActivityIndicator style={{ marginTop: 24 }} />}
      {(view === 'live' || view === 'gone') && saved?.length === 0 && <Notice text={tr('这一周电脑上的 Agent 都没报过额度')} />}
      {(view === 'live' || view === 'gone') && saved?.map((a) => <AgentCard key={a.agent} r={a} t={t} />)}
      {m.status === 'online' && <TodayTokens m={m} />}
    </View>
  );
}

export default function Quota() {
  const t = useTheme();
  const bottom = useBottom();
  const { machines } = useHub();
  const named = machines.length > 1;
  const only = named ? undefined : machines[0];

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav big title={tr('额度')} sub={only ? tr('来自 {name}', { name: only.name }) : undefined} />
      {machines.length === 0 && <Notice text={tr('还没有配对电脑')} />}
      <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 24, paddingBottom: bottom + 24 }}>
        {machines.map((m) => (
          <MachineQuota key={m.id} m={m} named={named} t={t} />
        ))}
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  card: { borderWidth: 1, borderRadius: 16, padding: 14, gap: 14 },
  row: { flexDirection: 'row', alignItems: 'baseline', justifyContent: 'space-between', gap: 8 },
  gauge: { flexDirection: 'row', gap: 2, height: 10 },
  stripe: { flex: 1, borderRadius: 1 },
});
