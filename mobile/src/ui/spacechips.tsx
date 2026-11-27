// One chip per space, 全部 first, to show one space alone: on the home screen and in search.
import { type ComponentRef, useCallback, useEffect, useRef } from 'react';
import { type LayoutChangeEvent, Pressable, ScrollView, StyleSheet, Text } from 'react-native';
import { tr } from '../i18n';
import { type Chip, revealX } from './inbox';
import { type Theme, useTheme } from './theme';

function SpaceChip({ chip, on, t, onPress, onLayout }: { chip: Chip; on: boolean; t: Theme; onPress: () => void; onLayout: (e: LayoutChangeEvent) => void }) {
  const label = chip.name ?? tr('全部');
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected: on }}
      accessibilityLabel={tr('{name}，{n} 个', { name: label, n: chip.count })}
      hitSlop={{ top: 7, bottom: 7 }}
      onPress={onPress}
      onLayout={onLayout}
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
  // The chosen chip in sight: a space picked elsewhere, or one past the edge, read 「moori」 with
  // the rest of its name and count off screen (2026-11-26 walk).
  const row = useRef<ComponentRef<typeof ScrollView>>(null);
  const at = useRef(new Map<string, { x: number; width: number }>());
  const view = useRef({ x: 0, width: 0 });
  const reveal = useCallback(() => {
    const c = at.current.get(filter ?? '');
    const x = c ? revealX(c, view.current.x, view.current.width, 18) : null;
    if (x !== null) row.current?.scrollTo({ x, animated: true });
  }, [filter]);
  useEffect(reveal, [reveal]);
  if (!chips.length) return null;
  return (
    <ScrollView
      ref={row}
      horizontal
      keyboardShouldPersistTaps="handled"
      showsHorizontalScrollIndicator={false}
      style={{ flexGrow: 0 }}
      contentContainerStyle={s.chips}
      onLayout={(e) => (view.current.width = e.nativeEvent.layout.width)}
      onScroll={(e) => (view.current.x = e.nativeEvent.contentOffset.x)}
      scrollEventThrottle={64}
    >
      {chips.map((chip) => (
        <SpaceChip
          key={chip.name ?? ''}
          chip={chip}
          on={chip.name === filter}
          t={t}
          onPress={() => onPick(chip.name)}
          onLayout={(e) => {
            at.current.set(chip.name ?? '', { x: e.nativeEvent.layout.x, width: e.nativeEvent.layout.width });
            if (chip.name === filter) reveal();
          }}
        />
      ))}
    </ScrollView>
  );
}

const s = StyleSheet.create({
  chips: { flexDirection: 'row', alignItems: 'center', gap: 6, paddingHorizontal: 18, paddingBottom: 8 },
  chip: { flexDirection: 'row', alignItems: 'center', gap: 5, height: 30, borderRadius: 15, borderWidth: 1, paddingHorizontal: 11, maxWidth: 180 },
  chipText: { fontSize: 13, flexShrink: 1 },
});
