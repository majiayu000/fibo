// One stretch of an Agent's work, call by call: what the summary line in the conversation
// folds. Fetched when opened and kept only while this screen is; a tool's text is never
// saved on the phone.
import { router, useLocalSearchParams } from 'expo-router';
import { memo, useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, FlatList, Pressable, StyleSheet, Text, View } from 'react-native';
import { NOT_YET, remote, supports, useHub } from '@/remote/hub';
import { Icon, type IconName } from '@/ui/icons';
import { Nav, Notice, mono, useBottom } from '@/ui/kit';
import type { Tools as Summary } from '@/ui/message';
import { span } from '@/ui/parse';
import { useTheme } from '@/ui/theme';
import { metaWords, type ToolCall } from '@/ui/toolview';

/** Rows of one height: 2,007 of them scroll as lightly as twenty. */
const ROW = 62;

const ICONS: Record<string, IconName> = { command: 'prompt', edit: 'diff', read: 'file', search: 'search', web: 'globe', agent: 'computer' };

const Row = memo(function Row({ call, onOpen }: { call: ToolCall; onOpen: (id: string) => void }) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${call.label || call.name}，${metaWords(call)}`}
      onPress={() => onOpen(call.id)}
      style={({ pressed }) => [s.row, { borderColor: t.sep }, pressed && { backgroundColor: t.well }]}
    >
      <Icon name={call.failed ? 'alert' : (ICONS[call.kind] ?? 'gear')} size={18} width={1.8} color={call.failed ? t.err : t.dim} />
      <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
        <Text numberOfLines={1} style={[s.label, { color: call.failed ? t.err : t.text }]}>
          {call.label || '（没有说明）'}
        </Text>
        <Text numberOfLines={1} style={{ fontSize: 12, color: call.failed ? t.err : t.dim }}>
          {metaWords(call)}
        </Text>
      </View>
      <Icon name="right" size={14} width={2} color={t.dim} />
    </Pressable>
  );
});

function summaryWords(tools: Summary | undefined, shown: number, more: boolean): string {
  if (!tools) return '';
  const calls = tools.calls ?? shown;
  return [tools.took ? `干了 ${span(tools.took)}` : '', `${more ? calls || shown : shown} 次调用`, tools.commands ? `${tools.commands} 个命令` : ''].filter(Boolean).join(' · ');
}

type Page = { tools?: Summary; calls: ToolCall[]; next?: number; gone?: boolean };

export default function ToolsScreen() {
  const t = useTheme();
  const bottom = useBottom();
  // No `seq`: the turn still going, since the last thing it said.
  const { machine, terminal, session, seq } = useLocalSearchParams<{ machine: string; terminal: string; session: string; seq?: string }>();
  const { machines } = useHub();
  const m = machines.find((x) => x.id === machine);
  const online = m?.status === 'online';
  const can = supports(m, 'toolcalls');
  const [calls, setCalls] = useState<ToolCall[]>([]);
  const [tools, setTools] = useState<Summary>();
  const [gone, setGone] = useState(false);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const next = useRef<number | undefined>(undefined);
  const busy = useRef(false);

  const page = useCallback(
    async (after?: number) => {
      const r = remote(machine);
      if (!r || busy.current) return;
      busy.current = true;
      try {
        const ask: Record<string, unknown> = seq ? { session, seq: Number(seq) } : { session };
        if (after !== undefined) ask.after = after;
        const p: Page = await r.request('toolcalls', ask);
        next.current = p.next;
        if (after === undefined) {
          setTools(p.tools);
          setCalls(p.calls);
        } else setCalls((have) => [...have, ...p.calls]);
        setGone(!!p.gone);
        setError('');
        // A page can be all scripts its commands stand in for: go on until something shows.
        busy.current = false;
        if (p.next !== undefined && p.calls.length === 0) await page(p.next);
      } catch (e) {
        setError((e as Error).message);
      } finally {
        busy.current = false;
        setLoading(false);
      }
    },
    [machine, session, seq],
  );

  useEffect(() => {
    if (online && can) page();
    else setLoading(false);
  }, [online, can, page]);

  const open = useCallback(
    (id: string) => router.push({ pathname: '/tool', params: { machine, session, call: id } }),
    [machine, session],
  );
  const renderItem = useCallback(({ item }: { item: ToolCall }) => <Row call={item} onOpen={open} />, [open]);
  const files = tools?.files ?? [];

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav rule title={seq ? '这段做了什么' : '它到现在做了什么'} sub={summaryWords(tools, calls.length, next.current !== undefined)} />
      {files.length > 0 && (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="看改动"
          onPress={() => router.push({ pathname: '/diff', params: { machine, terminal } })}
          style={({ pressed }) => [s.files, { borderColor: t.sep }, pressed && { backgroundColor: t.well }]}
        >
          <Icon name="diff" size={18} width={1.8} color={t.ok} />
          <Text numberOfLines={1} style={{ flex: 1, fontSize: 14, color: t.text }}>
            改了{' '}
            <Text style={{ fontFamily: mono, fontSize: 13, color: t.ok }}>
              {files.length === 1 ? files[0].path.split('/').pop() : `${files.length} 个文件`} +{files.reduce((n, f) => n + f.added, 0)} −{files.reduce((n, f) => n + f.removed, 0)}
            </Text>
          </Text>
          <Text style={{ fontSize: 13, color: t.dim }}>看改动</Text>
          <Icon name="right" size={14} width={2} color={t.dim} />
        </Pressable>
      )}
      {gone && <Notice text="原始记录已不在：只剩下这份清单，看不到每条的内容" />}
      {!online ? (
        <Notice text={m?.error ?? '电脑没连上'} />
      ) : !can ? (
        <Notice text={NOT_YET} />
      ) : error ? (
        <Notice text={error} />
      ) : loading ? (
        <ActivityIndicator style={{ marginTop: 40 }} />
      ) : (
        <FlatList
          data={calls}
          keyExtractor={(c) => c.id}
          renderItem={renderItem}
          getItemLayout={(_, i) => ({ length: ROW, offset: ROW * i, index: i })}
          initialNumToRender={14}
          maxToRenderPerBatch={14}
          windowSize={7}
          onEndReached={() => next.current !== undefined && page(next.current)}
          onEndReachedThreshold={1.5}
          contentContainerStyle={{ paddingBottom: bottom + 16 }}
          ListEmptyComponent={<Notice text="这一段没有工具调用" />}
        />
      )}
    </View>
  );
}

const s = StyleSheet.create({
  row: { height: ROW, flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 18, borderBottomWidth: 1 },
  label: { fontFamily: mono, fontSize: 13 },
  files: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 18, height: 50, borderBottomWidth: 1 },
});
