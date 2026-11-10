// The uncommitted changes where the conversation runs: one card per file, ticked 看过 on
// this phone, and a summary card with 要求修改 (a sentence to the Agent) and 让它提交
// (「提交吧」). A phone paired as 只看 or 能答审批 gets `summary`: what this turn changed,
// names and counts only, from the conversation's log (the computer runs no git for it).
import { useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { oops } from '@/actions';
import { NOT_YET, remote, supports, useHub, write } from '@/remote/hub';
import { useReviewed } from '@/store/reviewed';
import { Icon } from '@/ui/icons';
import { Btn, Nav, Notice, TITLE_WEIGHT, mono, useBottom } from '@/ui/kit';
import { type Check, checkLine, diffLines, diffWords } from '@/ui/parse';
import { SayDialog } from '@/ui/sheets';
import { tilde } from '@/ui/status';
import { type Theme, useTheme } from '@/ui/theme';
import { notUi, tr } from '@/i18n';

type File = { path: string; added: number; removed: number; status: string; patch: string; truncated?: boolean };
type Diff = { cwd: string; branch?: string; files: File[]; added: number; removed: number; truncated?: boolean; checks?: Check[]; summary?: boolean };

function FileCard({ f, t, seen, onTick, open, onOpen, names }: { f: File; t: Theme; seen: boolean; onTick: () => void; open: boolean; onOpen: () => void; names: boolean }) {
  // Names only: nothing to open.
  open = open && !names;
  const lines = open ? diffLines(f.patch) : [];
  return (
    <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
      <Pressable disabled={names} onPress={onOpen} style={[s.fh, open && { borderBottomWidth: 1, borderColor: t.sep }]}>
        {!names && <Icon name={open ? 'down' : 'right'} size={14} width={2} color={t.text} />}
        <Text numberOfLines={1} style={[s.path, { color: t.text }]}>
          {f.path}
        </Text>
        {f.status !== 'M' && <Text style={[s.monoSmall, { color: t.dim }]}>{f.status === '?' || f.status === 'A' ? tr('新') : f.status}</Text>}
        <Text style={[s.monoSmall, { color: t.ok }]}>+{f.added}</Text>
        <Text style={[s.monoSmall, { color: t.err }]}>−{f.removed}</Text>
        <Pressable
          accessibilityRole="checkbox"
          accessibilityLabel={tr('标为看过')}
          accessibilityState={{ checked: seen }}
          hitSlop={10}
          onPress={onTick}
          style={[s.chk, { borderColor: seen ? t.act : t.dim, backgroundColor: seen ? t.act : 'transparent' }]}
        >
          {seen && <Icon name="check" size={13} width={3} color={t.onAct} />}
        </Pressable>
      </Pressable>
      {open && (
        <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={{ flexGrow: 1 }}>
          <View style={{ flexGrow: 1, paddingBottom: 4 }}>
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
            {f.truncated && <Text style={[s.dl, { color: t.dim, paddingLeft: 12 }]}>{tr('…太长，只显示了前面一部分')}</Text>}
            {!f.patch && <Text style={[s.dl, { color: t.dim, paddingLeft: 12 }]}>{tr('没有可显示的文本改动')}</Text>}
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
  const check = checkLine(diff?.checks);
  const seen = files.filter((f) => reviewed.has(f.path, f.patch)).length;
  const words = diffWords(diff);
  const say = (text: string) =>
    write(machine, 'send', { terminal, text, enter: true })
      .then(() => setSent(text))
      .catch((e) => oops((e as Error).message));

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav
        big
        title={words.title}
        sub={
          diff ? (
            <Text style={{ fontSize: 12, color: t.dim }} numberOfLines={1}>
              <Text style={{ color: t.ok }}>+{diff.added}</Text> <Text style={{ color: t.err }}>−{diff.removed}</Text>
              {` · ${tr('{n} 个文件', { n: files.length })}${diff.branch ? ` · ${diff.branch}` : ''}`}
            </Text>
          ) : undefined
        }
      />
      {!online ? (
        <Notice text={m?.error ?? tr('电脑没连上')} />
      ) : !can ? (
        <Notice text={tr(NOT_YET)} />
      ) : error ? (
        <Notice text={error} />
      ) : !diff ? (
        <ActivityIndicator style={{ marginTop: 40 }} />
      ) : (
        <>
          <ScrollView contentContainerStyle={{ paddingHorizontal: 12, paddingTop: 4, paddingBottom: 16, gap: 10 }}>
            {words.note !== '' && <Text style={{ fontSize: 12, color: t.dim, paddingHorizontal: 4 }}>{words.note}</Text>}
            {files.length === 0 && <Notice text={words.empty} />}
            {files.map((f) => (
              <FileCard
                key={f.path}
                f={f}
                t={t}
                seen={reviewed.has(f.path, f.patch)}
                onTick={() => reviewed.toggle(f.path, f.patch)}
                open={!!openSet[f.path]}
                onOpen={() => setOpen((o) => ({ ...o, [f.path]: !o[f.path] }))}
                names={!!diff.summary}
              />
            ))}
            {diff.truncated && <Notice text={tr('改动太多，只列出了一部分')} />}
          </ScrollView>
          {files.length > 0 && (
            <View style={[s.sheet, s.sum, { backgroundColor: t.card, borderColor: t.line, marginBottom: bottom + 8 }]}>
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
                <View style={[s.okIcon, { backgroundColor: seen === files.length ? t.okBg : t.well }]}>
                  <Icon name="check" size={18} width={2.2} color={seen === files.length ? t.ok : t.dim} />
                </View>
                <View style={{ gap: 1, flex: 1 }}>
                  <Text style={{ fontSize: 16, fontWeight: TITLE_WEIGHT, color: t.text }}>
                    {tr('看过 {seen} / {n} 个文件', { seen, n: files.length })}
                  </Text>
                  <Text numberOfLines={2} style={{ fontSize: 12, color: !sent && check?.failed && !check.stale ? t.err : t.dim }}>
                    {sent ? tr('已发给它：{text}', { text: sent }) : (check?.text ?? tilde(diff.cwd))}
                  </Text>
                </View>
              </View>
              {terminal && supports(m, 'send') && (
                <View style={{ flexDirection: 'row', gap: 8 }}>
                  <Btn tall label={tr('要求修改')} onPress={() => setAsking(true)} />
                  <Btn tall primary label={tr('让它提交')} onPress={() => say(notUi('提交吧'))} />
                </View>
              )}
            </View>
          )}
        </>
      )}
      <SayDialog
        visible={asking}
        title={tr('要求修改')}
        placeholder={tr('比如：偏移量别写死，取标题栏的高度')}
        action={tr('发给它')}
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
