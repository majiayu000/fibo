// The paired computers: online or not, how this phone reaches them (the relay's host),
// the latency this phone measured from its own pings, its permission, their system. An
// offline one can be retried or removed; the relays they go through are listed below.
import { router } from 'expo-router';
import { ScrollView, StyleSheet, Text, View } from 'react-native';
import { confirm } from '@/actions';
import { type Machine, forget, reconnect, useHub } from '@/remote/hub';
import { Btn, Label, Nav, Round, Wide, useBottom } from '@/ui/kit';
import { relayPlace } from '@/ui/parse';
import { type Theme, ago, useTheme } from '@/ui/theme';

const ROLE = { view: '只看', answer: '能答审批', full: '完全控制' } as const;
const OS: Record<string, string> = { macos: 'macOS', linux: 'Linux', windows: 'Windows' };

async function remove(m: Machine) {
  const ok = await confirm(`从手机上移除「${m.name || '电脑'}」？`, '要再连这台电脑得重新扫码配对。电脑那边的配对记录不受影响。', '移除', true);
  if (ok) forget(m.id);
}

function Stat({ k, v, unit, t }: { k: string; v: string; unit?: string; t: Theme }) {
  return (
    <View style={{ flex: 1, gap: 2 }}>
      <Text style={{ fontSize: 12, color: t.dim }}>{k}</Text>
      <Text style={{ fontSize: 19, fontWeight: '600', color: t.text }} numberOfLines={1}>
        {v}
        {!!unit && <Text style={{ fontSize: 13, color: t.dim, fontWeight: '400' }}>{` ${unit}`}</Text>}
      </Text>
    </View>
  );
}

function MachineCard({ m, t }: { m: Machine; t: Theme }) {
  const place = relayPlace(m.link.relay);
  const online = m.status === 'online';
  const conversations = m.cards.filter((c) => c.agent && c.phase !== 'shell').length;
  const facts = [m.os ? (OS[m.os] ?? m.os) : '', m.role ? `权限：${ROLE[m.role]}` : '', m.appVersion ? `Mooring ${m.appVersion}` : ''].filter(Boolean);
  return (
    <View
      style={[
        s.card,
        { backgroundColor: online ? t.card : t.bg, borderColor: t.line },
        !online && { borderStyle: 'dashed' },
      ]}
    >
      <View style={s.top}>
        <View style={[s.dot, { backgroundColor: online ? t.ok : m.status === 'refused' ? t.err : t.dim }]} />
        <Text style={[s.name, { color: online ? t.text : t.dim }]} numberOfLines={1}>
          {m.name || '电脑'}
        </Text>
        <Text style={{ marginLeft: 'auto', fontSize: 13, color: online ? t.ok : t.dim, fontWeight: online ? '600' : '400' }}>
          {online ? '在线' : m.status === 'connecting' ? '正在连接' : m.seen ? `${ago(m.seen)}在线` : '不在线'}
        </Text>
      </View>
      {online ? (
        <>
          <View style={{ flexDirection: 'row', gap: 8 }}>
            <Stat t={t} k="对话" v={String(conversations)} />
            <Stat t={t} k="连接" v={place.lan ? '局域网' : '中继'} />
            <Stat t={t} k="延迟" v={m.latency === undefined ? '—' : m.latency < 1 ? '<1' : String(m.latency)} unit={m.latency !== undefined ? 'ms' : undefined} />
          </View>
          <Text style={{ fontSize: 13, color: t.dim }} numberOfLines={2}>
            {[...facts, `经 ${place.host}`].join(' · ')}
          </Text>
        </>
      ) : (
        <>
          {!!m.error && (
            <Text style={{ fontSize: 13, color: m.status === 'refused' ? t.err : t.dim, lineHeight: 19 }}>{m.error}</Text>
          )}
          {m.status !== 'connecting' && (
            <View style={{ flexDirection: 'row', gap: 8 }}>
              <View style={{ flex: 1 }}>
                <Wide kind="grey" height={40} label="重试" onPress={() => reconnect(m.id)} textStyle={{ fontSize: 15 }} />
              </View>
              <Wide kind="danger" height={40} label="移除" onPress={() => remove(m)} textStyle={{ fontSize: 15 }} />
            </View>
          )}
        </>
      )}
    </View>
  );
}

export default function Machines() {
  const t = useTheme();
  const bottom = useBottom();
  const { machines } = useHub();
  const relays = [...new Set(machines.map((m) => relayPlace(m.link.relay).host))];
  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav
        big
        title="电脑"
        right={
          <View style={{ flexDirection: 'row', gap: 8 }}>
            <Btn icon="plus" label="配对" onPress={() => router.push('/pair')} style={{ height: 44, borderRadius: 22 }} />
            <Round icon="gear" label="设置" onPress={() => router.push('/settings')} />
          </View>
        }
      />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 14, paddingBottom: bottom + 16 }}>
        {machines.length === 0 && <Text style={{ color: t.dim, textAlign: 'center', paddingTop: 40 }}>还没有配对电脑</Text>}
        {machines.map((m) => (
          <MachineCard key={m.id} m={m} t={t} />
        ))}
        {relays.length > 0 && <Label style={{ marginTop: 8 }}>中继</Label>}
        {relays.map((host) => {
          const using = machines.filter((m) => relayPlace(m.link.relay).host === host);
          const up = using.some((m) => m.status === 'online');
          return (
            <View key={host} style={[s.card, { backgroundColor: t.card, borderColor: t.line, flexDirection: 'row', alignItems: 'center' }]}>
              <View style={{ flex: 1, gap: 3 }}>
                <Text style={{ fontSize: 16, color: t.text }} numberOfLines={1}>
                  {host}
                </Text>
                <Text style={{ fontSize: 13, color: t.dim }}>端到端加密，中继看不到内容 · {using.length} 台电脑</Text>
              </View>
              <Text style={{ fontSize: 13, fontWeight: '600', color: up ? t.ok : t.dim }}>{up ? '正常' : '没连上'}</Text>
            </View>
          );
        })}
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  card: { borderWidth: 1, borderRadius: 16, padding: 14, gap: 12 },
  top: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  dot: { width: 8, height: 8, borderRadius: 4 },
  name: { fontSize: 17, fontWeight: '600', flexShrink: 1 },
});
