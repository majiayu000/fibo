// The board: the desktop's five columns, one at a time — 要你处理, 在跑, 停下了, 待办, 做完了, 很久没动
// (desktop only for now), 今天完成 — conversations grouped by space within each.
import { router } from 'expo-router';
import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { type Card, type Machine, useHub } from '@/remote/hub';
import { cardTitle } from '@/ui/format';
import { Icon, SpaceDot } from '@/ui/icons';
import { Nav, Notice, mono, useBottom } from '@/ui/kit';
import { spaceName } from '@/ui/row';
import { agentName, needsYou, statusColor, statusKey, statusWord, tilde } from '@/ui/status';
import { type Theme, ago, useTheme } from '@/ui/theme';

type Column = 'you' | 'run' | 'review' | 'todo' | 'done' | 'stale';
const COLUMNS: { key: Column; label: string }[] = [
  { key: 'you', label: '要你处理' },
  { key: 'run', label: '在跑' },
  { key: 'review', label: '停下了' },
  { key: 'todo', label: '待办' },
  { key: 'done', label: '做完了' },
  { key: 'stale', label: '很久没动' },
];

/** The column a card goes in: the same reading of what the host said as the desktop's
 *  `remote_column`. */
function column(c: Card): Column | null {
  if (!c.session) return null;
  if (c.outcome === 'done') return 'done';
  if (c.outcome === 'abandoned' || c.outcome === 'deferred') return null;
  const k = statusKey(c);
  if (k === 'fresh' || k === 'closed' || k === 'deferred') return null;
  if (k === 'running') return 'run';
  if (needsYou(c) || c.approval) return 'you';
  return k === 'stale' ? 'stale' : 'review';
}

const colour = (k: Column, t: Theme) => ({ you: t.wait, run: t.run, review: t.dim, todo: t.dim, done: t.dim, stale: t.dim })[k];

type Item = { m: Machine; c: Card };

export default function Board() {
  const t = useTheme();
  const bottom = useBottom();
  const { machines } = useHub();
  const [only, setOnly] = useState<string | null>(null);
  const [col, setCol] = useState<Column>('you');
  const shown = machines.filter((m) => !only || m.id === only);
  const cols: Record<Column, Item[]> = { you: [], run: [], review: [], todo: [], done: [], stale: [] };
  for (const m of shown) for (const c of m.cards) {
    if (!c.agent || c.phase === 'shell') continue;
    const k = column(c);
    if (k) cols[k].push({ m, c });
  }
  const total = COLUMNS.reduce((n, x) => n + cols[x.key].length, 0);
  const items = cols[col].sort((a, b) => (b.c.updated ?? 0) - (a.c.updated ?? 0));
  const groups = new Map<string, Item[]>();
  for (const it of items) {
    const name = spaceName(it.m, it.c);
    groups.set(name, [...(groups.get(name) ?? []), it]);
  }
  const filter = machines.find((m) => m.id === only);
  const nextFilter = () => {
    const i = machines.findIndex((m) => m.id === only);
    setOnly(i + 1 < machines.length ? machines[i + 1].id : null);
  };

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav
        big
        title="看板"
        right={
          machines.length > 1 ? (
            <Pressable onPress={nextFilter} style={[s.filter, { borderColor: t.line, backgroundColor: t.card }]}>
              <Text style={{ fontSize: 14, color: t.text }}>{filter?.name ?? '全部电脑'}</Text>
              <Icon name="down" size={14} width={2} color={t.text} />
            </Pressable>
          ) : undefined
        }
      />
      <View style={[s.strip, { paddingHorizontal: 18 }]}>
        {COLUMNS.map((x) => (
          <View key={x.key} style={{ flex: total ? Math.max(cols[x.key].length, 0.4) : 1, height: 4, borderRadius: 2, backgroundColor: cols[x.key].length ? colour(x.key, t) : t.empty }} />
        ))}
      </View>
      <View>
        <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={{ paddingHorizontal: 18, gap: 8, paddingVertical: 10 }}>
          {COLUMNS.map((x) => {
            const on = x.key === col;
            return (
              <Pressable
                key={x.key}
                onPress={() => setCol(x.key)}
                style={[s.tab, { borderColor: on ? t.act : t.line, backgroundColor: on ? t.act : t.card }]}
              >
                <View style={[s.dot, { backgroundColor: colour(x.key, t) }]} />
                <Text style={{ fontSize: 14, color: on ? t.onAct : t.text }}>{x.label}</Text>
                {x.key !== 'todo' && <Text style={{ fontSize: 14, fontWeight: '700', color: on ? t.onAct : t.text }}>{cols[x.key].length}</Text>}
              </Pressable>
            );
          })}
        </ScrollView>
      </View>
      <ScrollView contentContainerStyle={{ paddingBottom: bottom + 16 }}>
        {col === 'todo' ? (
          <Notice text="待办只在电脑上：桌面端的待办还没进远程协议" />
        ) : items.length === 0 ? (
          <Notice text="这一列现在是空的" />
        ) : (
          [...groups].map(([name, list]) => (
            <View key={name}>
              <View style={s.grp}>
                <SpaceDot id={list[0].c.space || name} size={10} />
                <Text style={{ fontSize: 15, fontWeight: '600', color: t.text }}>{name}</Text>
                <Text style={{ fontSize: 14, color: t.dim }}>· {list.length}</Text>
              </View>
              {list.map(({ m, c }) => (
                <Pressable
                  key={`${m.id}/${c.id}`}
                  onPress={() => router.push({ pathname: '/chat', params: { machine: m.id, terminal: c.id } })}
                  style={({ pressed }) => [s.row, { borderColor: t.sep }, pressed && { backgroundColor: t.well }]}
                >
                  <View style={{ flexDirection: 'row', alignItems: 'baseline', gap: 8 }}>
                    <Text numberOfLines={1} style={{ flex: 1, fontSize: 16, color: t.text }}>
                      {cardTitle(c)}
                    </Text>
                    <Text style={{ fontSize: 13, color: t.dim }}>{c.updated ? ago(c.updated) : ''}</Text>
                  </View>
                  {col === 'you' ? (
                    <Text numberOfLines={1} style={{ fontSize: 14, color: statusColor(c, t) }}>
                      {c.approval ? '等你审批' : (c.waiting || statusWord(c)).split('\n')[0]}
                    </Text>
                  ) : (
                    <Text numberOfLines={1} style={{ fontFamily: mono, fontSize: 13, color: t.dim }}>
                      {tilde(c.cwd)}
                    </Text>
                  )}
                  <Text numberOfLines={1} style={{ fontSize: 13, color: t.dim }}>
                    {[agentName(c.agent), m.name].filter(Boolean).join(' · ')}
                  </Text>
                </Pressable>
              ))}
            </View>
          ))
        )}
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  filter: { height: 44, borderRadius: 22, borderWidth: 1, flexDirection: 'row', alignItems: 'center', gap: 6, paddingHorizontal: 14 },
  strip: { flexDirection: 'row', gap: 3, paddingTop: 6 },
  tab: { height: 36, borderRadius: 18, borderWidth: 1, flexDirection: 'row', alignItems: 'center', gap: 7, paddingHorizontal: 14 },
  dot: { width: 8, height: 8, borderRadius: 4 },
  grp: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 18, paddingTop: 14, paddingBottom: 6 },
  row: { gap: 4, paddingVertical: 12, paddingHorizontal: 18, borderBottomWidth: 1 },
});
