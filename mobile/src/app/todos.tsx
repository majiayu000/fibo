// 待办: what you wrote down and have not closed, by space, with how far each got. What runs
// now is the home screen's; the conversations nobody touched for days fold in at the foot.
import { router } from 'expo-router';
import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { supports, useHub } from '@/remote/hub';
import { cardTitle } from '@/ui/format';
import { foldLabel } from '@/ui/inbox';
import { SpaceDot } from '@/ui/icons';
import { Group, Nav, Notice, Round } from '@/ui/kit';
import { agentName } from '@/ui/status';
import { TabBar } from '@/ui/tabs';
import { type Theme, ago, useTheme } from '@/ui/theme';
import { StateWord } from '@/ui/todoWord';
import { type TodoItem, lastNote, shelveTodos, todoKey, touched } from '@/ui/todos';

function Row({ item, t }: { item: TodoItem; t: Theme }) {
  const { m, todo } = item;
  const note = lastNote(todo);
  const at = touched(todo);
  return (
    <Pressable
      accessibilityRole="button"
      onPress={() => router.push({ pathname: '/todo', params: { machine: m.id, id: todo.id } })}
      style={({ pressed }) => [s.row, { borderColor: t.sep }, pressed && { backgroundColor: t.well }]}
    >
      <View style={s.top}>
        <Text numberOfLines={2} style={{ flex: 1, fontSize: 16, lineHeight: 22, color: t.text }}>
          {todo.title}
        </Text>
        <StateWord todo={todo} t={t} />
      </View>
      {!!note && (
        <Text numberOfLines={2} style={{ fontSize: 13, lineHeight: 19, color: t.dim }}>
          {note}
        </Text>
      )}
      <Text numberOfLines={1} style={{ fontSize: 13, color: t.dim }}>
        {[agentName(todo.agent), at ? ago(at) : ''].filter(Boolean).join(' · ')}
      </Text>
    </Pressable>
  );
}

export default function Todos() {
  const t = useTheme();
  const { machines } = useHub();
  const [space, setSpace] = useState<string | null>(null);
  const [staleOpen, setStaleOpen] = useState(false);
  const shelf = shelveTodos(machines, space);
  const canAdd = machines.some((m) => supports(m, 'todoadd'));

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav big tab title={shelf.total ? `待办 · ${shelf.total}` : '待办'} right={canAdd ? <Round icon="plus" label="记一件待办" onPress={() => router.push('/todonew')} /> : undefined} />
      {shelf.chips.length > 0 && (
        <View>
          <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={s.chips}>
            {shelf.chips.map((c) => {
              const on = c.space === shelf.filter;
              return (
                <Pressable
                  key={c.space ?? ''}
                  accessibilityRole="button"
                  accessibilityState={{ selected: on }}
                  onPress={() => setSpace(c.space)}
                  style={[s.chip, { borderColor: on ? t.act : t.line, backgroundColor: on ? t.act : t.card }]}
                >
                  <Text style={{ fontSize: 14, color: on ? t.onAct : t.text }}>{c.space ?? '全部'}</Text>
                  <Text style={{ fontSize: 14, fontWeight: '700', color: on ? t.onAct : t.text }}>{c.count}</Text>
                </Pressable>
              );
            })}
          </ScrollView>
        </View>
      )}
      <ScrollView contentContainerStyle={{ paddingBottom: 16 }}>
        {shelf.groups.length === 0 && <Notice text={machines.length ? '没有没收尾的待办' : '还没有配对电脑'} />}
        {shelf.groups.map((g) => (
          <View key={g.space}>
            <View style={s.grp}>
              <SpaceDot id={g.items[0].todo.space || g.space} size={10} />
              <Text style={{ fontSize: 15, fontWeight: '600', color: t.text }}>{g.space}</Text>
              <Text style={{ fontSize: 14, color: t.dim }}>· {g.items.length}</Text>
            </View>
            {g.items.map((i) => (
              <Row key={todoKey(i)} item={i} t={t} />
            ))}
          </View>
        ))}
        {shelf.stale.length > 0 && (
          <Group label={foldLabel(shelf.stale)} open={staleOpen} onPress={() => setStaleOpen(!staleOpen)} />
        )}
        {staleOpen &&
          shelf.stale.map(({ m, c }) => (
            <Pressable
              key={`${m.id}/${c.id}`}
              accessibilityRole="button"
              onPress={() => router.push({ pathname: '/chat', params: { machine: m.id, terminal: c.id } })}
              style={({ pressed }) => [s.row, { borderColor: t.sep }, pressed && { backgroundColor: t.well }]}
            >
              <Text numberOfLines={1} style={{ fontSize: 16, color: t.text }}>
                {cardTitle(c)}
              </Text>
              <Text numberOfLines={1} style={{ fontSize: 13, color: t.dim }}>
                {[agentName(c.agent), c.updated ? ago(c.updated) : ''].filter(Boolean).join(' · ')}
              </Text>
            </Pressable>
          ))}
      </ScrollView>
      <TabBar on="todos" />
    </View>
  );
}

const s = StyleSheet.create({
  chips: { paddingHorizontal: 18, gap: 8, paddingVertical: 10 },
  chip: { height: 36, borderRadius: 18, borderWidth: 1, flexDirection: 'row', alignItems: 'center', gap: 7, paddingHorizontal: 14 },
  grp: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 18, paddingTop: 14, paddingBottom: 6 },
  row: { gap: 4, paddingVertical: 12, paddingHorizontal: 18, borderBottomWidth: 1 },
  top: { flexDirection: 'row', alignItems: 'flex-start', gap: 8 },
});
