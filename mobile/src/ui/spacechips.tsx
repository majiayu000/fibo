// One chip per space, 全部 first, to show one space alone: on the home screen and in search.
import { Pressable, ScrollView, StyleSheet, Text } from 'react-native';
import type { Chip } from './inbox';
import { type Theme, useTheme } from './theme';

function SpaceChip({ chip, on, t, onPress }: { chip: Chip; on: boolean; t: Theme; onPress: () => void }) {
  const label = chip.name ?? '全部';
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected: on }}
      accessibilityLabel={`${label}，${chip.count} 个`}
      hitSlop={{ top: 7, bottom: 7 }}
      onPress={onPress}
      style={({ pressed }) => [s.chip, { borderColor: on ? t.act : t.line, backgroundColor: on ? t.act : t.card }, pressed && { opacity: 0.7 }]}
    >
      <Text numberOfLines={1} style={[s.chipText, { color: on ? t.onAct : t.text }]}>
        {label}
      </Text>
      {chip.count > 0 && <Text style={[s.chipText, { color: on ? t.onAct : t.dim }]}>{chip.count}</Text>}
    </Pressable>
  );
}

export function SpaceChips({ chips, filter, onPick }: { chips: Chip[]; filter: string | null; onPick: (space: string | null) => void }) {
  const t = useTheme();
  if (!chips.length) return null;
  return (
    <ScrollView horizontal keyboardShouldPersistTaps="handled" showsHorizontalScrollIndicator={false} style={{ flexGrow: 0 }} contentContainerStyle={s.chips}>
      {chips.map((chip) => (
        <SpaceChip key={chip.name ?? ''} chip={chip} on={chip.name === filter} t={t} onPress={() => onPick(chip.name)} />
      ))}
    </ScrollView>
  );
}

const s = StyleSheet.create({
  chips: { flexDirection: 'row', alignItems: 'center', gap: 6, paddingHorizontal: 18, paddingBottom: 8 },
  chip: { flexDirection: 'row', alignItems: 'center', gap: 5, height: 30, borderRadius: 15, borderWidth: 1, paddingHorizontal: 11, maxWidth: 180 },
  chipText: { fontSize: 13, flexShrink: 1 },
});
