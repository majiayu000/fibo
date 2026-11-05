// One tool call whole: what was asked and what came back, as the machine cleaned them. A long
// text comes as its two ends and the middle is fetched on request; all of it goes when this
// screen does, never into storage.
import * as Clipboard from 'expo-clipboard';
import { useLocalSearchParams } from 'expo-router';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { NOT_YET, remote, supports, useHub } from '@/remote/hub';
import { CodeView } from '@/ui/codeview';
import { Btn, Nav, Notice, mono, useBottom } from '@/ui/kit';
import { type Theme, useTheme } from '@/ui/theme';
import { allText, byteLength, bytes, extend, loaded, type Loaded, metaWords, missing, type ToolDetail, toolName, viewRows } from '@/ui/toolview';

type Part = 'input' | 'output';
type Texts = { input: Loaded; output?: Loaded };
/** Bytes each text has reached from its start: the machine's offsets. */
type Ends = { input: number; output: number };

function Tab({ label, on, onPress, t }: { label: string; on: boolean; onPress: () => void; t: Theme }) {
  return (
    <Pressable
      accessibilityRole="tab"
      accessibilityState={{ selected: on }}
      onPress={onPress}
      style={[s.tab, on && { backgroundColor: t.card, borderColor: t.line }, !on && { borderColor: 'transparent' }]}
    >
      <Text numberOfLines={1} style={{ fontSize: 13, color: on ? t.text : t.dim }}>
        {label}
      </Text>
    </Pressable>
  );
}

export default function ToolScreen() {
  const t = useTheme();
  const bottom = useBottom();
  const { machine, session, call } = useLocalSearchParams<{ machine: string; session: string; call: string }>();
  const { machines } = useHub();
  const m = machines.find((x) => x.id === machine);
  const online = m?.status === 'online';
  const can = supports(m, 'toolcall');
  const [detail, setDetail] = useState<ToolDetail>();
  const [texts, setTexts] = useState<Texts>();
  const [ends, setEnds] = useState<Ends>({ input: 0, output: 0 });
  const [part, setPart] = useState<Part>('output');
  const [error, setError] = useState('');
  const [fetching, setFetching] = useState(false);
  const [note, setNote] = useState('');
  const left = useRef(false);

  useEffect(() => {
    left.current = false;
    return () => {
      left.current = true;
    };
  }, []);

  useEffect(() => {
    const r = online && can ? remote(machine) : null;
    if (!r) return;
    let live = true;
    r.request('toolcall', { session, call })
      .then((d: ToolDetail) => {
        if (!live) return;
        setDetail(d);
        setTexts({ input: loaded(d.input), output: d.output && loaded(d.output) });
        setEnds({ input: byteLength(d.input.head), output: d.output ? byteLength(d.output.head) : 0 });
        if (!d.output) setPart('input');
        setError('');
      })
      .catch((e) => live && setError((e as Error).message));
    return () => {
      live = false;
    };
  }, [online, can, machine, session, call]);

  const text = texts && (part === 'output' ? texts.output : texts.input);
  const end = ends[part];

  /** The next stretch of the part on screen; with `all`, on until nothing is missing. */
  const fetchMore = useCallback(
    async (all: boolean) => {
      const r = remote(machine);
      if (!r || fetching) return;
      setFetching(true);
      try {
        let at = end;
        let now = text;
        while (now && missing(now, at) > 0 && !left.current) {
          const piece = await r.request('toolcall', { session, call, part, offset: at });
          const next: Loaded = extend(now, piece.text);
          at = piece.next as number;
          now = next;
          const reached = at;
          setTexts((old) => old && { ...old, [part]: next });
          setEnds((old) => ({ ...old, [part]: reached }));
          if (!all) break;
        }
      } catch (e) {
        setError((e as Error).message);
      } finally {
        setFetching(false);
      }
    },
    [machine, session, call, part, end, text, fetching],
  );

  const rows = useMemo(() => (text ? viewRows(text, end, part === 'input' ? (detail?.input_kind ?? '') : 'text') : []), [text, end, part, detail?.input_kind]);
  const rest = text ? missing(text, end) : 0;
  const gap = useCallback(
    () => (
      <Text style={{ fontSize: 13, color: t.dim, textAlign: 'center' }} numberOfLines={1}>
        {`… 中间还有 ${bytes(rest)} 没取 …`}
      </Text>
    ),
    [t, rest],
  );

  const copy = () => {
    if (!text) return;
    Clipboard.setStringAsync(allText(text)).catch(() => undefined);
    setNote(rest > 0 ? `已复制取到的部分，中间还差 ${bytes(rest)}` : '已复制全部');
    setTimeout(() => !left.current && setNote(''), 3000);
  };

  const steps = detail?.steps ?? [];
  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav
        rule
        title={detail ? toolName(detail.name) : '工具调用'}
        sub={
          detail ? (
            <Text numberOfLines={1} style={{ fontSize: 12, color: detail.failed ? t.err : t.dim }}>
              {metaWords(detail, false)}
            </Text>
          ) : undefined
        }
      />
      {!online ? (
        <Notice text={m?.error ?? '电脑没连上'} />
      ) : !can ? (
        <Notice text={NOT_YET} />
      ) : error && !detail ? (
        <Notice text={error} />
      ) : !detail || !texts ? (
        <ActivityIndicator style={{ marginTop: 40 }} />
      ) : (
        <>
          {!!detail.about && (
            <Text selectable style={{ marginHorizontal: 18, marginBottom: 10, fontSize: 15, lineHeight: 22, color: t.text }}>
              {detail.about}
            </Text>
          )}
          {!!detail.label && (
            <Text numberOfLines={3} selectable style={[s.well, { backgroundColor: t.well, color: detail.failed ? t.err : t.text }]}>
              {detail.label}
            </Text>
          )}
          {!!detail.note && <Notice text={detail.note} style={{ paddingVertical: 4, textAlign: 'left' }} />}
          {steps.length > 0 && (
            <ScrollView style={{ maxHeight: 96, marginHorizontal: 18, marginBottom: 8 }}>
              {steps.map((st, i) => (
                <Text key={i} numberOfLines={1} style={{ fontFamily: mono, fontSize: 12, lineHeight: 19, color: st.unread ? t.dim : t.text }}>
                  {i + 1}. {st.label}
                </Text>
              ))}
            </ScrollView>
          )}
          <View style={s.bar}>
            <View style={[s.tabs, { backgroundColor: t.well }]} accessibilityRole="tablist">
              <Tab t={t} on={part === 'output'} onPress={() => setPart('output')} label={detail.output ? `输出 ${bytes(detail.size ?? detail.output.total)}` : '输出'} />
              <Tab t={t} on={part === 'input'} onPress={() => setPart('input')} label={`输入 ${bytes(detail.input.total)}`} />
            </View>
            <Btn label="复制" onPress={copy} disabled={!text} />
          </View>
          {rest > 0 && (
            <View style={s.more}>
              <Text style={{ flex: 1, fontSize: 13, color: t.dim }} numberOfLines={1}>
                {fetching ? '正在取…' : `中间还有 ${bytes(rest)}`}
              </Text>
              <Btn label="再取一段" disabled={fetching} onPress={() => fetchMore(false)} />
              <Btn label="全部取完" disabled={fetching} onPress={() => fetchMore(true)} />
            </View>
          )}
          {!!(note || error) && <Notice text={note || error} tone={error ? 'danger' : undefined} style={{ paddingVertical: 2 }} />}
          {part === 'output' && !detail.output ? (
            <Notice text="还没有结果：可能还没跑完，也可能结果没有写进记录" />
          ) : text && !text.total ? (
            <Notice text={part === 'output' ? '没有输出' : '没有输入'} />
          ) : (
            <CodeView rows={rows} gap={gap} footer={bottom + 24} />
          )}
        </>
      )}
    </View>
  );
}

const s = StyleSheet.create({
  well: { fontFamily: mono, fontSize: 13, lineHeight: 19, marginHorizontal: 18, marginTop: 10, marginBottom: 8, padding: 12, borderRadius: 12, overflow: 'hidden' },
  bar: { flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 18, paddingVertical: 6 },
  tabs: { flex: 1, flexDirection: 'row', borderRadius: 18, padding: 3, gap: 2 },
  more: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 18, paddingBottom: 6 },
  tab: { flex: 1, height: 30, borderRadius: 15, borderWidth: 1, alignItems: 'center', justifyContent: 'center' },
});
