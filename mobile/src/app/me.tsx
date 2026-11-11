// 我的: the computers and how much of each Agent they have used, then settings. A computer that
// is off takes one line here, so it never stands between you and what needs you on the home screen.
import { router } from 'expo-router';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { type Machine, useHub } from '@/remote/hub';
import { tk, tr } from '@/i18n';
import { useSettings } from '@/store/settings';
import { Icon } from '@/ui/icons';
import { Label, Nav } from '@/ui/kit';
import { relayPlace } from '@/ui/parse';
import { Bar, tightWords, useTightWindows } from '@/ui/quotaLine';
import { TabBar } from '@/ui/tabs';
import { type Theme, useTheme } from '@/ui/theme';

const APPEARANCE = { system: tk('跟随系统'), light: tk('浅色'), dark: tk('深色') } as const;

function Line({ t, title, value, onPress, last }: { t: Theme; title: string; value?: string; onPress: () => void; last?: boolean }) {
  return (
    <Pressable accessibilityRole="button" onPress={onPress} style={({ pressed }) => [s.line, { borderColor: t.sep }, last && { borderBottomWidth: 0 }, pressed && { backgroundColor: t.well }]}>
      <Text numberOfLines={1} style={{ fontSize: 15, color: t.text, flexShrink: 1 }}>
        {title}
      </Text>
      <View style={s.lineEnd}>
        {!!value && <Text style={{ fontSize: 13, color: t.dim }}>{value}</Text>}
        <Icon name="right" size={14} width={2} color={t.dim} />
      </View>
    </Pressable>
  );
}

function OnlineCard({ m, t }: { m: Machine; t: Theme }) {
  const tight = useTightWindows([m]);
  return (
    <View style={[s.card, { backgroundColor: t.card, borderColor: t.line }]}>
      <Pressable accessibilityRole="button" onPress={() => router.push('/machines')} style={s.cardHead}>
        <View style={[s.dot, { backgroundColor: t.ok }]} />
        <Text numberOfLines={1} style={{ fontSize: 15, fontWeight: '600', color: t.text, flexShrink: 1 }}>
          {m.name || tr('电脑')}
        </Text>
        <Text style={{ marginLeft: 'auto', fontSize: 13, color: t.dim }}>{relayPlace(m.link.relay).lan ? tr('局域网') : tr('中继')}</Text>
      </Pressable>
      {tight.length > 0 && (
        <Pressable accessibilityRole="button" accessibilityLabel={tr('看全部额度')} onPress={() => router.push('/quota')} style={{ gap: 10 }}>
          {tight.map((q) => (
            <View key={q.agent} style={{ gap: 5 }}>
              <Text style={{ fontSize: 13, color: t.text }}>{tightWords(q, false)}</Text>
              <Bar used={q.used} t={t} />
            </View>
          ))}
        </Pressable>
      )}
    </View>
  );
}

export default function Me() {
  const t = useTheme();
  const { machines } = useHub();
  const v = useSettings();
  const online = machines.filter((m) => m.status === 'online');
  const off = machines.length - online.length;
  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav big tab title={tr('我的')} />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, paddingBottom: 24, gap: 10 }}>
        <View style={s.head}>
          <Label>{tr('电脑#list')}</Label>
          <Pressable accessibilityRole="button" hitSlop={10} onPress={() => router.push('/pair')}>
            <Text style={{ fontSize: 14, color: t.link }}>{tr('配对新电脑')}</Text>
          </Pressable>
        </View>
        {machines.length === 0 && <Text style={{ fontSize: 14, color: t.dim }}>{tr('还没有配对电脑')}</Text>}
        {online.map((m) => (
          <OnlineCard key={m.id} m={m} t={t} />
        ))}
        {off > 0 && (
          <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
            <Line t={t} title={tr('{n} 台电脑离线', { n: off })} value={tr('查看或移除')} last onPress={() => router.push('/machines')} />
          </View>
        )}
        <Label style={{ marginTop: 14 }}>{tr('设置')}</Label>
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
          <Line t={t} title={tr('额度明细')} onPress={() => router.push('/quota')} />
          <Line t={t} title={tr('外观')} value={tr(APPEARANCE[v.appearance])} onPress={() => router.push('/settings')} />
          <Line t={t} title={tr('打开时要 Face ID / 指纹')} value={v.lock ? tr('开') : tr('关')} onPress={() => router.push('/settings')} />
          <Line t={t} title={tr('通知')} value={tr('推送还没接')} last onPress={() => router.push('/settings')} />
        </View>
      </ScrollView>
      <TabBar on="me" />
    </View>
  );
}

const s = StyleSheet.create({
  head: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  card: { borderWidth: 1, borderRadius: 16, padding: 14, gap: 12 },
  cardHead: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  dot: { width: 8, height: 8, borderRadius: 4 },
  sheet: { borderWidth: 1, borderRadius: 16, overflow: 'hidden' },
  line: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 10, minHeight: 50, paddingHorizontal: 14, borderBottomWidth: 1 },
  lineEnd: { flexDirection: 'row', alignItems: 'center', gap: 6 },
});
