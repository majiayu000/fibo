// A conversation as one flat row (the design's new look): project and computer, its
// state in one coloured word on the right; the title; then branch (else directory) and
// what it is doing (else its Agent) in mono.
import type { ReactNode } from 'react';
import { Pressable, type StyleProp, StyleSheet, Text, type TextStyle, View } from 'react-native';
import type { Card, Machine } from '../remote/hub';
import { cardTitle } from './format';
import { isFull } from './modes';
import { SpaceMark } from './icons';
import { TITLE_WEIGHT, mono } from './kit';
import { useNow } from './now';
import { runningFor } from './parse';
import { rowDetail, spaceName, statusColor, statusWord, tilde } from './status';
import { useTheme } from './theme';

export { spaceName };

/** 「在跑 · 12 分钟」. Its own component: the clock redraws this word, not the row. */
export function RunningWord({ c, color, style }: { c: Card; color: string; style?: StyleProp<TextStyle> }) {
  const time = runningFor(c.running_since, useNow());
  return <Text style={[style, { color }]}>{time ? `${statusWord(c)} · ${time}` : statusWord(c)}</Text>;
}

export function Row({
  m,
  c,
  onPress,
  children,
  right,
  dim,
  last,
}: {
  m: Machine;
  c: Card;
  onPress?: () => void;
  /** Under the title instead of the mono line: a command and buttons. */
  children?: ReactNode;
  /** Replaces the state word. */
  right?: { text: string; color?: string };
  dim?: boolean;
  last?: boolean;
}) {
  const t = useTheme();
  const stale = m.status !== 'online';
  const word = right?.text ?? (stale ? `断开前：${statusWord(c)}` : statusWord(c));
  const color = right ? (right.color ?? t.dim) : stale ? t.dim : statusColor(c, t);
  const space = spaceName(m, c);
  return (
    <Pressable
      // With buttons inside, the row must not swallow them for screen readers.
      accessible={!children}
      onPress={onPress}
      style={({ pressed }) => [
        s.row,
        { borderColor: t.sep, backgroundColor: t.bg },
        last && { borderBottomWidth: 0 },
        pressed && onPress && { backgroundColor: t.well },
      ]}
    >
      <View style={[s.l1, (stale || dim) && { opacity: 0.6 }]}>
        <SpaceMark name={space} id={c.space || space} />
        <Text numberOfLines={1} style={[s.where, { color: t.dim }]}>
          {space} · {m.name || '电脑'}
        </Text>
        {!right && !stale && c.running_since ? <RunningWord c={c} color={color} style={s.state} /> : <Text style={[s.state, { color }]}>{word}</Text>}
      </View>
      <Text numberOfLines={2} style={[s.title, { color: dim ? t.dim : t.text }, stale && { opacity: 0.6 }]}>
        {cardTitle(c)}
      </Text>
      {children ?? (
        <Text numberOfLines={1} style={[s.l3, { color: t.dim }, stale && { opacity: 0.6 }]}>
          {[isFull(c.permission) ? c.permission : '', c.branch || tilde(c.cwd), rowDetail(c)].filter(Boolean).join(' · ')}
        </Text>
      )}
    </Pressable>
  );
}

export const rowStyles = StyleSheet.create({
  l1: { flexDirection: 'row', alignItems: 'center', gap: 7 },
  where: { fontSize: 13, flexShrink: 1 },
  state: { marginLeft: 'auto', fontSize: 13, paddingLeft: 8 },
  title: { fontSize: 16, fontWeight: TITLE_WEIGHT, lineHeight: 22 },
});

const s = StyleSheet.create({
  row: { gap: 3, paddingVertical: 12, paddingHorizontal: 18, borderBottomWidth: 1 },
  ...rowStyles,
  l3: { fontFamily: mono, fontSize: 12 },
});
