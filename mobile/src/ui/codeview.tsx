// A long tool text: monospace rows of one height in a list that draws only what is on
// screen, sliding sideways over the longest line as one block.
import { memo, useCallback, useMemo } from 'react';
import { FlatList, Platform, ScrollView, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import { mono } from './mono';
import { useTheme } from './theme';
import { columns, type ViewRow } from './toolview';

const LINE = 19;
const GAP = 64;
const CHAR = 7.2;
const PAD = 18;

const Line = memo(function Line({ text, tone, width }: { text: string; tone?: 'add' | 'del'; width: number }) {
  const t = useTheme();
  return (
    <View style={[{ height: LINE, width, paddingHorizontal: PAD }, tone === 'add' && { backgroundColor: t.okBg }, tone === 'del' && { backgroundColor: t.errBg }]}>
      <Text numberOfLines={1} ellipsizeMode="clip" style={[s.line, { color: t.text }]}>
        {text || ' '}
      </Text>
    </View>
  );
});

export function CodeView({ rows, gap, footer }: { rows: ViewRow[]; gap: () => React.ReactElement; footer?: number }) {
  const { width: screen } = useWindowDimensions();
  const width = useMemo(() => Math.max(screen, columns(rows) * CHAR + PAD * 2), [rows, screen]);
  const gapAt = useMemo(() => rows.findIndex((r) => r.kind === 'gap'), [rows]);
  const renderItem = useCallback(
    ({ item }: { item: ViewRow }) =>
      item.kind === 'gap' ? <View style={{ height: GAP, width: screen, justifyContent: 'center', paddingHorizontal: PAD }}>{gap()}</View> : <Line text={item.text} tone={item.tone} width={width} />,
    [gap, screen, width],
  );
  const list = (
    <FlatList
      style={{ width }}
      data={rows}
      keyExtractor={(_, i) => String(i)}
      renderItem={renderItem}
      getItemLayout={(_, i) => {
        const before = gapAt >= 0 && i > gapAt;
        return { length: i === gapAt ? GAP : LINE, offset: LINE * i + (before ? GAP - LINE : 0), index: i };
      }}
      initialNumToRender={40}
      maxToRenderPerBatch={40}
      windowSize={5}
      contentContainerStyle={{ paddingTop: 8, paddingBottom: footer ?? 24 }}
    />
  );
  return width > screen ? (
    <ScrollView horizontal showsHorizontalScrollIndicator style={{ flex: 1 }} contentContainerStyle={{ flexGrow: 1 }}>
      {list}
    </ScrollView>
  ) : (
    <View style={{ flex: 1 }}>{list}</View>
  );
}

const s = StyleSheet.create({
  line: {
    fontFamily: mono,
    fontSize: 12,
    lineHeight: LINE,
    includeFontPadding: false,
    ...(Platform.OS === 'web' ? ({ whiteSpace: 'pre' } as object) : {}),
  },
});
