// A Markdown or plain-text file a chat links to, read from the computer the conversation
// runs on (its `file` op: only a fully controlling phone, only in the conversation's folder).
// Read only. When it cannot be read, it says why; the path can always be copied.
import * as Clipboard from 'expo-clipboard';
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator, ScrollView, Text, View } from 'react-native';
import { NOT_YET, remote, supports, useHub } from '@/remote/hub';
import { Btn, Nav, Notice, mono, useBottom } from '@/ui/kit';
import { Markdown } from '@/ui/markdown';
import { useTheme } from '@/ui/theme';
import { tr } from '@/i18n';

type Read = { path: string; text: string; markdown: boolean; truncated?: boolean };

export default function FileScreen() {
  const t = useTheme();
  const bottom = useBottom();
  const { machine, terminal, path } = useLocalSearchParams<{ machine: string; terminal: string; path: string }>();
  const { machines } = useHub();
  const m = machines.find((x) => x.id === machine);
  const online = m?.status === 'online';
  const can = supports(m, 'file');
  const [read, setRead] = useState<Read | null>(null);
  const [error, setError] = useState('');
  const [copied, setCopied] = useState('');

  useEffect(() => {
    const r = online && can ? remote(machine) : null;
    if (!r) return;
    let live = true;
    r.request('file', { terminal, path })
      .then((f: Read) => live && (setRead(f), setError('')))
      .catch((e) => live && setError((e as Error).message));
    return () => {
      live = false;
    };
  }, [online, can, machine, terminal, path]);

  // A lesser pairing is told so; an older computer lists no `file` for any phone.
  const why = !online
    ? (m?.error ?? tr('电脑没连上'))
    : !can
      ? m?.role && m.role !== 'full'
        ? tr('这台手机的配对权限不能读电脑上的文件')
        : tr(NOT_YET)
      : error;
  const full = read?.path ?? path;
  const copy = () => {
    Clipboard.setStringAsync(full).catch(() => undefined);
    setCopied(tr('已复制路径 {path}', { path: full }));
  };
  const name = full.split('/').pop() || full;
  // A link in the file is from the file's folder, not the conversation's.
  const near = (p: string) => (/^[/~]/.test(p) || !read ? p : `${read.path.replace(/[^/]*$/, '')}${p}`);

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav big title={name} sub={full} />
      {why ? (
        <Notice text={why} />
      ) : !read ? (
        <ActivityIndicator style={{ marginTop: 40 }} />
      ) : (
        <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 4, paddingBottom: 16, gap: 10 }}>
          {read.markdown ? (
            <Markdown text={read.text} t={t} color={t.text} fold={false} onFile={(p) => router.push({ pathname: '/file', params: { machine, terminal, path: near(p) } })} />
          ) : (
            <Text selectable style={{ fontFamily: mono, fontSize: 13, lineHeight: 20, color: t.text }}>
              {read.text}
            </Text>
          )}
          {read.truncated && <Notice text={tr('…太长，只显示了前面一部分')} />}
        </ScrollView>
      )}
      <View style={{ paddingHorizontal: 12, paddingTop: 8, paddingBottom: bottom + 8, gap: 6 }}>
        {copied !== '' && <Text style={{ fontSize: 12, color: t.dim }}>{copied}</Text>}
        <Btn label={tr('复制电脑路径')} onPress={copy} style={{ minHeight: 44, justifyContent: 'center' }} />
      </View>
    </View>
  );
}
