// Quota on the home screen: a row of its own under the title, one chip per Agent, each with
// its fullest window. It sat at the end of the 在跑 fold showing only the fullest Agent, and
// read as part of that fold (2026-11-09). A tap opens 额度.
import { router } from 'expo-router';
import { Pressable, ScrollView, StyleSheet, Text } from 'react-native';
import { type Tight, level, windowName } from './parse';
import { Bar } from './quotaLine';
import { agentName } from './status';
import { useTheme } from './theme';

export function QuotaStrip({ chips, named }: { chips: Tight[]; named: boolean }) {
  const t = useTheme();
  if (!chips.length) return null;
  return (
    <ScrollView horizontal showsHorizontalScrollIndicator={false} style={s.strip} contentContainerStyle={s.row}>
      {chips.map((q) => (
        <Pressable
          key={`${q.machine}\n${q.agent}`}
          accessibilityRole="button"
          accessibilityLabel={`${named ? `${q.machine} ` : ''}${agentName(q.agent)} ${windowName(q.minutes)}用了 ${q.used}%，看全部额度`}
          onPress={() => router.push('/quota')}
          style={({ pressed }) => [s.chip, pressed && { backgroundColor: t.well }]}
        >
          <Text numberOfLines={1} style={{ fontSize: 12, color: level(q.used) === 'high' ? t.err : t.dim }}>
            {named ? `${q.machine} · ` : ''}
            {agentName(q.agent)} {q.used}%
          </Text>
          <Bar used={q.used} t={t} width={24} />
        </Pressable>
      ))}
    </ScrollView>
  );
}

const s = StyleSheet.create({
  // A horizontal ScrollView grows down the column unless told not to (it took half the screen).
  strip: { flexGrow: 0 },
  row: { paddingHorizontal: 18, gap: 12, alignItems: 'center' },
  chip: { flexDirection: 'row', alignItems: 'center', gap: 6, minHeight: 28, borderRadius: 8 },
});
