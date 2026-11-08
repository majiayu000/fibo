// One todo: your words, what done means, the progress written back, newest first. 接着做 goes
// to the conversation working on it, or starts its Agent for it; 标记完成 is yours, with an undo.
import { router, useLocalSearchParams } from 'expo-router';
import { useRef, useState } from 'react';
import { ScrollView, StyleSheet, Text, View } from 'react-native';
import { oops } from '@/actions';
import { type Todo, refresh, supports, useHub, write } from '@/remote/hub';
import { Nav, Notice, Wide, useBottom } from '@/ui/kit';
import { agentName } from '@/ui/status';
import { ago, useTheme } from '@/ui/theme';
import { todoSpace } from '@/ui/todos';
import { StateWord } from '@/ui/todoWord';

export default function TodoScreen() {
  const t = useTheme();
  const { machine, id } = useLocalSearchParams<{ machine: string; id: string }>();
  const { machines } = useHub();
  const m = machines.find((x) => x.id === machine);
  const live = m?.todos?.find((x) => x.id === id);
  // Closed, it leaves the list: the screen keeps what it showed, to undo from.
  const seen = useRef<Todo | undefined>(undefined);
  if (live) seen.current = live;
  const todo = live ?? seen.current;
  const [closed, setClosed] = useState(false);
  const [busy, setBusy] = useState(false);
  const bottom = useBottom();

  if (!m || !todo) {
    return (
      <View style={{ flex: 1, backgroundColor: t.bg }}>
        <Nav title="待办" />
        <Notice text="这个待办不在了" />
      </View>
    );
  }
  const canClose = supports(m, 'todooutcome');
  // A computer that knows `todoadd` also starts a todo when asked to open one.
  const canStart = !!todo.terminal || (supports(m, 'open') && supports(m, 'todoadd'));
  const notes = [...(todo.notes ?? [])].reverse();

  async function settle(outcome: 'done' | 'open') {
    setBusy(true);
    try {
      await write(m!.id, 'todooutcome', { task: todo!.id, outcome });
      setClosed(outcome === 'done');
      await refresh(m!.id);
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function work() {
    if (todo!.terminal) {
      router.push({ pathname: '/chat', params: { machine: m!.id, terminal: todo!.terminal } });
      return;
    }
    setBusy(true);
    try {
      const ok = await write(m!.id, 'open', { task: todo!.id });
      await refresh(m!.id);
      if (ok?.terminal) router.push({ pathname: '/chat', params: { machine: m!.id, terminal: ok.terminal } });
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav rule title="待办" right={closed ? undefined : <StateWord todo={todo} t={t} />} />
      <ScrollView contentContainerStyle={{ paddingBottom: 16 }}>
        <View style={{ paddingHorizontal: 18, paddingTop: 16, gap: 4 }}>
          <Text selectable style={{ fontSize: 19, lineHeight: 27, fontWeight: '600', color: t.text }}>
            {todo.title}
          </Text>
          <Text style={{ fontSize: 13, color: t.dim }}>
            {[todoSpace(m, todo), agentName(todo.agent), todo.created ? `记于 ${ago(todo.created)}` : ''].filter(Boolean).join(' · ')}
          </Text>
        </View>
        {!!todo.said && (
          <>
            <Text style={[s.sec, { color: t.dim }]}>你说的</Text>
            <Text selectable style={[s.box, { backgroundColor: t.well, color: t.text }]}>
              {todo.said}
            </Text>
          </>
        )}
        {(todo.acceptance?.length ?? 0) > 0 && (
          <>
            <Text style={[s.sec, { color: t.dim }]}>怎样算做完</Text>
            <View style={[s.box, { borderWidth: 1, borderColor: t.line, gap: 6 }]}>
              {todo.acceptance!.map((a, i) => (
                <Text key={i} selectable style={{ fontSize: 15, lineHeight: 22, color: t.text }}>
                  {todo.acceptance!.length > 1 ? `· ${a}` : a}
                </Text>
              ))}
            </View>
          </>
        )}
        <Text style={[s.sec, { color: t.dim }]}>{notes.length ? `进展 · ${notes.length}` : '进展'}</Text>
        {notes.length === 0 && <Text style={{ paddingHorizontal: 18, fontSize: 14, color: t.dim }}>还没有 Agent 写回进展</Text>}
        {notes.map((n, i) => (
          <View key={i} style={[s.note, { borderColor: t.sep }]}>
            <Text selectable style={{ fontSize: 14, lineHeight: 21, color: t.text }}>
              {n.text}
            </Text>
            <Text style={{ fontSize: 12, color: t.dim }}>{ago(n.at)}</Text>
          </View>
        ))}
      </ScrollView>
      {closed ? (
        <View style={[s.btns, { borderColor: t.sep, paddingBottom: bottom + 12 }]}>
          <Text style={{ flex: 1, fontSize: 15, color: t.text }}>已标记完成</Text>
          <Wide kind="outline" label="撤销" disabled={busy} onPress={() => settle('open')} height={44} />
        </View>
      ) : (
        (canClose || canStart) && (
          <View style={[s.btns, { borderColor: t.sep, paddingBottom: bottom + 12 }]}>
            {canClose && (
              <View style={{ flex: 1 }}>
                <Wide kind="outline" label="标记完成" disabled={busy} onPress={() => settle('done')} />
              </View>
            )}
            {canStart && (
              <View style={{ flex: 1 }}>
                <Wide label="接着做" disabled={busy} onPress={work} />
              </View>
            )}
          </View>
        )
      )}
    </View>
  );
}

const s = StyleSheet.create({
  sec: { paddingHorizontal: 18, paddingTop: 18, paddingBottom: 6, fontSize: 13 },
  box: { marginHorizontal: 18, padding: 12, borderRadius: 14, fontSize: 15, lineHeight: 22, overflow: 'hidden' },
  note: { marginHorizontal: 18, paddingVertical: 10, borderBottomWidth: 1, gap: 3 },
  btns: { flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 18, paddingTop: 12, borderTopWidth: 1 },
});
