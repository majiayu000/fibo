// Where the folder list is, above it: one chip per level with a folder icon, the current one
// filled, the middle folded when the path is long, scrolled so the current level is in sight
// (it started at the root and cut the current folder off the screen, 2026-11-09).
import { type ComponentRef, useRef, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { Icon } from './icons';
import { pathChips } from './parse';
import { useTheme } from './theme';

export function PathBar({ crumbs, onGo }: { crumbs: { label: string; path: string }[]; onGo: (path: string) => void }) {
  const t = useTheme();
  const bar = useRef<ComponentRef<typeof ScrollView>>(null);
  const [open, setOpen] = useState(false);
  const chips = pathChips(crumbs);
  const go = (path: string) => {
    setOpen(false);
    onGo(path);
  };
  return (
    <View style={{ gap: 6 }}>
      <ScrollView
        ref={bar}
        horizontal
        showsHorizontalScrollIndicator={false}
        onContentSizeChange={() => bar.current?.scrollToEnd({ animated: false })}
        contentContainerStyle={s.row}
      >
        {chips.map((c, i) => (
          <View key={c.kind === 'more' ? 'more' : c.path} style={s.row}>
            {i > 0 && <Icon name="right" size={12} width={2} color={t.dim} />}
            {c.kind === 'more' ? (
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={`中间还有 ${c.hidden.length} 级`}
                onPress={() => setOpen((o) => !o)}
                style={[s.chip, { borderColor: t.line, backgroundColor: open ? t.well : t.card }]}
              >
                <Text style={{ color: t.dim, fontSize: 13 }}>… {c.hidden.length} 级</Text>
              </Pressable>
            ) : (
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={c.current ? `现在在 ${c.label}` : `去 ${c.label}`}
                disabled={c.current}
                onPress={() => go(c.path)}
                style={({ pressed }) => [
                  s.chip,
                  c.current ? { borderColor: 'transparent', backgroundColor: t.well } : { borderColor: t.line, backgroundColor: pressed ? t.well : t.card },
                ]}
              >
                <Icon name={c.root && c.label === '电脑' ? 'computer' : 'folder'} size={15} width={1.8} color={t.link} />
                <Text numberOfLines={1} style={{ color: t.text, fontSize: 13, fontWeight: c.current ? '600' : '400', maxWidth: 180 }}>
                  {c.label}
                </Text>
              </Pressable>
            )}
          </View>
        ))}
      </ScrollView>
      {open && chips[1]?.kind === 'more' && (
        <View style={[s.menu, { borderColor: t.line, backgroundColor: t.card }]}>
          {chips[1].hidden.map((h) => (
            <Pressable key={h.path} accessibilityRole="button" onPress={() => go(h.path)} style={({ pressed }) => [s.item, pressed && { backgroundColor: t.well }]}>
              <Icon name="folder" size={15} width={1.8} color={t.link} />
              <Text numberOfLines={1} style={{ flex: 1, color: t.text, fontSize: 14 }}>
                {h.label}
              </Text>
            </Pressable>
          ))}
        </View>
      )}
    </View>
  );
}

const s = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  chip: { flexDirection: 'row', alignItems: 'center', gap: 5, height: 32, paddingHorizontal: 10, borderRadius: 10, borderWidth: 1 },
  menu: { borderWidth: 1, borderRadius: 12, paddingVertical: 4 },
  item: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 12, minHeight: 40 },
});
