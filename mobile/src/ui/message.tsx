// A conversation's lines in the new look: your words in a grey bubble on the right, the
// Agent's flat across the width, and the work between them folded into one line.
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { Icon } from './icons';
import { mono } from './kit';
import { Markdown } from './markdown';
import { span } from './parse';
import { clock, useTheme } from './theme';

export type Tools = { commands?: number; files?: { path: string; added: number; removed: number }[]; took?: number };
export type Message = { seq: number; at: number; role: 'user' | 'agent'; text: string; tools?: Tools | null };

export function Mine({ text, at, note, tone }: { text: string; at?: number; note?: string; tone?: 'pending' | 'error' }) {
  const t = useTheme();
  return (
    <View style={{ gap: 6 }}>
      <View style={[s.mine, { backgroundColor: t.bubble }, tone === 'pending' && { opacity: 0.6 }]}>
        <Text selectable style={{ color: t.text, fontSize: 15, lineHeight: 22 }}>
          {text}
        </Text>
      </View>
      <Text style={[s.meta, { color: tone === 'error' ? t.err : t.dim, textAlign: 'right' }]}>{note ?? (at ? clock(at) : '')}</Text>
    </View>
  );
}

/** A stretch this long says so even with no command or edit to show: it read and thought. */
const LONG_S = 60;

/** 「干了 2 分钟 · 跑了 3 个命令 · 改了 chat.tsx +6 −2」, opening the diff. */
export function Worked({ tools, onPress }: { tools: Tools; onPress?: () => void }) {
  const t = useTheme();
  const files = tools.files ?? [];
  const commands = tools.commands ?? 0;
  const took = tools.took ?? 0;
  if (!commands && !files.length && took < LONG_S) return null;
  const added = files.reduce((n, f) => n + f.added, 0);
  const removed = files.reduce((n, f) => n + f.removed, 0);
  const name = files.length === 1 ? files[0].path.split('/').pop() : `${files.length} 个文件`;
  return (
    <Pressable onPress={onPress} disabled={!onPress} style={[s.worked, { borderColor: t.sep }]}>
      <Text numberOfLines={1} style={{ color: t.dim, fontSize: 13, flex: 1 }}>
        {[took > 0 ? `干了 ${span(took)}` : '', commands > 0 ? `跑了 ${commands} 个命令` : ''].filter(Boolean).join(' · ')}
        {(took > 0 || commands > 0) && files.length > 0 && ' · '}
        {files.length > 0 && '改了 '}
        {files.length > 0 && (
          <Text style={{ fontFamily: mono, color: t.ok }}>
            {name} +{added} −{removed}
          </Text>
        )}
      </Text>
      {onPress && <Icon name="right" size={14} width={2} color={t.dim} />}
    </Pressable>
  );
}

export function Theirs({ m, agent, onTools }: { m: Message; agent: string; onTools?: () => void }) {
  const t = useTheme();
  return (
    <View style={{ gap: 8 }}>
      {m.tools && <Worked tools={m.tools} onPress={onTools} />}
      <Markdown text={m.text} t={t} color={t.text} />
      <Text style={[s.meta, { color: t.dim }]}>{[clock(m.at), agent].filter(Boolean).join(' · ')}</Text>
    </View>
  );
}

const s = StyleSheet.create({
  mine: { alignSelf: 'flex-end', maxWidth: '80%', borderRadius: 20, paddingVertical: 11, paddingHorizontal: 15 },
  meta: { fontSize: 12 },
  worked: { flexDirection: 'row', alignItems: 'center', gap: 6, paddingVertical: 8, borderBottomWidth: 1 },
});
