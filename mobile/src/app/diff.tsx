// The uncommitted changes where the conversation runs: one card per file, ticked 看过 on
// this phone, and a summary card with 要求修改 (a sentence to the Agent) and 让它提交
// (「提交吧」).
import { useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { oops } from '@/actions';
import { NOT_YET, remote, supports, useHub, write } from '@/remote/hub';
import { useReviewed } from '@/store/reviewed';
import { Icon } from '@/ui/icons';
import { Btn, Nav, Notice, TITLE_WEIGHT, mono, useBottom } from '@/ui/kit';
import { diffLines } from '@/ui/parse';
import { SayDialog } from '@/ui/sheets';
import { type Theme, useTheme } from '@/ui/theme';

type File = { path: string; added: number; removed: number; status: string; patch: string; truncated?: boolean };
type Diff = { cwd: string; branch?: string; files: File[]; added: number; removed: number; truncated?: boolean };

function FileCard({ f, t, seen, onTick, open, onOpen }: { f: File; t: Theme; seen: boolean; onTick: () => void; open: boolean; onOpen: () => void }) {
  const lines = open ? diffLines(f.patch) : [];
  return (
    <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
      <Pressable onPress={onOpen} style={[s.fh, open && { borderBottomWidth: 1, borderColor: t.sep }]}>
        <Icon name={open ? 'down' : 'right'} size={14} width={2} color={t.text} />
        <Text numberOfLines={1} style={[s.path, { color: t.text }]}>
          {f.path}
        </Text>
        {f.status !== 'M' && <Text style={[s.monoSmall, { color: t.dim }]}>{f.status === '?' ? '新' : f.status}</Text>}
        <Text style={[s.monoSmall, { color: t.ok }]}>+{f.added}</Text>
        <Text style={[s.monoSmall, { color: t.err }]}>−{f.removed}</Text>
        <Pressable
          accessibilityRole="checkbox"
          accessibilityLabel="标为看过"
          accessibilityState={{ checked: seen }}
          hitSlop={10}
          onPress={onTick}
          style={[s.chk, { borderColor: seen ? t.act : t.dim, backgroundColor: seen ? t.act : 'transparent' }]}
        >
          {seen && <Icon name="check" size={13} width={3} color={t.onAct} />}
        </Pressable>
      </Pressable>
      {open && (
        <ScrollView horizontal showsHorizontalScrollIndicator={false}>
          <View style={{ minWidth: '100%', paddingBottom: 4 }}>
            {lines.map((l, i) =>
              l.kind === 'hunk' ? (
                <Text key={i} style={[s.dl, { color: t.dim, paddingLeft: 40 }]}>
                  {l.text}
                </Text>
              ) : (
                <View key={i} style={[s.row, l.kind === 'add' && { backgroundColor: t.okBg }, l.kind === 'del' && { backgroundColor: t.errBg }]}>
                  <Text style={[s.dl, s.no, { color: t.dim }]}>{l.no}</Text>
                  <Text style={[s.dl, { color: t.text }]}>
                    {l.kind === 'add' ? '+ ' : l.kind === 'del' ? '- ' : '  '}
                    {l.text}
                  </Text>
                </View>
              ),
            )}
            {f.truncated && <Text style={[s.dl, { color: t.dim, paddingLeft: 12 }]}>…太长，只显示了前面一部分</Text>}
            {!f.patch && <Text style={[s.dl, { color: t.dim, paddingLeft: 12 }]}>没有可显示的文本改动</Text>}
          </View>
        </ScrollView>
      )}
    </View>
  );
}

export default function DiffScreen() {
  const t = useTheme();
  const bottom = useBottom();
  const { machine, terminal, cwd } = useLocalSearchParams<{ machine: string; terminal?: string; cwd?: string }>();
  const { machines } = useHub();
  const m = machines.find((x) => x.id === machine);
  const online = m?.status === 'online';
  const can = supports(m, 'diff');
  const [diff, setDiff] = useState<Diff | null>(null);
  const [error, setError] = useState('');
  const [openSet, setOpen] = useState<Record<string, boolean>>({});
  const [asking, setAsking] = useState(false);
  const [sent, setSent] = useState('');
  const reviewed = useReviewed(machine, diff?.cwd ?? '');

  useEffect(() => {
    const r = online && can ? remote(machine) : null;
    if (!r) return;
    let live = true;
    r.request('diff', terminal ? { terminal } : { cwd })
      .then((d: Diff) => {
        if (!live) return;
        setDiff(d);
        setError('');
        // The first file starts open, the rest folded.
        if (d.files[0]) setOpen((o) => (Object.keys(o).length ? o : { [d.files[0].path]: true }));
      })
      .catch((e) => live && setError((e as Error).message));
    return () => {
      live = false;
    };
  }, [online, can, machine, terminal, cwd]);

  const files = diff?.files ?? [];
  const seen = files.filter((f) => reviewed.has(f.path, f.patch)).length;
  const say = (text: string) =>
    write(machine, 'send', { terminal, text, enter: true })
      .then(() => setSent(text))
      .catch((e) => oops((e as Error).message));

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav
        big
        title="未提交的改动"
        sub={
          diff ? (
            <Text style={{ fontSize: 12, color: t.dim }} numberOfLines={1}>
              <Text style={{ color: t.ok }}>+{diff.added}</Text> <Text style={{ color: t.err }}>−{diff.removed}</Text>
              {` · ${files.length} 个文件${diff.branch ? ` · ${diff.branch}` : ''}`}
            </Text>
          ) : undefined
        }
      />
      {!online ? (
        <Notice text={m?.error ?? '电脑没连上'} />
      ) : !can ? (
        <Notice text={NOT_YET} />
      ) : error ? (
        <Notice text={error} />
      ) : !diff ? (
        <ActivityIndicator style={{ marginTop: 40 }} />
      ) : (
        <>
          <ScrollView contentContainerStyle={{ paddingHorizontal: 12, paddingTop: 4, paddingBottom: 16, gap: 10 }}>
            {files.length === 0 && <Notice text="没有未提交的改动" />}
            {files.map((f) => (
              <FileCard
                key={f.path}
                f={f}
                t={t}
                seen={reviewed.has(f.path, f.patch)}
                onTick={() => reviewed.toggle(f.path, f.patch)}
                open={!!openSet[f.path]}
                onOpen={() => setOpen((o) => ({ ...o, [f.path]: !o[f.path] }))}
              />
            ))}
            {diff.truncated && <Notice text="改动太多，只列出了一部分" />}
          </ScrollView>
          {files.length > 0 && (
            <View style={[s.sheet, s.sum, { backgroundColor: t.card, borderColor: t.line, marginBottom: bottom + 8 }]}>
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
                <View style={[s.okIcon, { backgroundColor: seen === files.length ? t.okBg : t.well }]}>
                  <Icon name="check" size={18} width={2.2} color={seen === files.length ? t.ok : t.dim} />
                </View>
                <View style={{ gap: 1, flex: 1 }}>
                  <Text style={{ fontSize: 16, fontWeight: TITLE_WEIGHT, color: t.text }}>
                    看过 {seen} / {files.length} 个文件
                  </Text>
                  <Text numberOfLines={1} style={{ fontSize: 12, color: t.dim }}>
                    {sent ? `已发给它：${sent}` : diff.cwd}
                  </Text>
                </View>
              </View>
              {terminal && supports(m, 'send') && (
                <View style={{ flexDirection: 'row', gap: 8 }}>
                  <Btn tall label="要求修改" onPress={() => setAsking(true)} />
                  <Btn tall primary label="让它提交" onPress={() => say('提交吧')} />
                </View>
              )}
            </View>
          )}
        </>
      )}
      <SayDialog
        visible={asking}
        title="要求修改"
        placeholder="比如：偏移量别写死，取标题栏的高度"
        action="发给它"
        onClose={() => setAsking(false)}
        onSend={(text) => {
          setAsking(false);
          say(text);
        }}
      />
    </View>
  );
}

const s = StyleSheet.create({
  sheet: { borderWidth: 1, borderRadius: 20, overflow: 'hidden' },
  fh: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingVertical: 10, paddingHorizontal: 12 },
  path: { fontFamily: mono, fontSize: 12, flexShrink: 1 },
  monoSmall: { fontFamily: mono, fontSize: 12 },
  chk: { width: 20, height: 20, borderRadius: 6, borderWidth: 1.5, marginLeft: 'auto', alignItems: 'center', justifyContent: 'center' },
  row: { flexDirection: 'row', gap: 10, paddingHorizontal: 12 },
  dl: { fontFamily: mono, fontSize: 12, lineHeight: 21 },
  no: { width: 28, textAlign: 'right' },
  sum: { marginHorizontal: 12, padding: 14, gap: 12 },
  okIcon: { width: 36, height: 36, borderRadius: 10, alignItems: 'center', justifyContent: 'center' },
});
