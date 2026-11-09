// A conversation's lines in the new look: your words in a grey bubble on the right, the
// Agent's flat across the width, and the work between them folded into one line.
import * as Clipboard from 'expo-clipboard';
import { memo, useEffect, useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { Icon } from './icons';
import { mono } from './kit';
import { Markdown } from './markdown';
import { readable } from './md/parse';
import { span } from './parse';
import { SelectSheet } from './sheets';
import { clock, useTheme } from './theme';

export type Tools = { commands?: number; files?: { path: string; added: number; removed: number }[]; took?: number; calls?: number };
export type Message = { seq: number; at: number; role: 'user' | 'agent'; text: string; tools?: Tools | null };

const NOTE_MS = 3000;

/** What a long press leaves behind: a note in the meta line for a moment, and copying done. */
function useCopied() {
  const [note, setNote] = useState('');
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  const say = useRef((text: string) => {
    setNote(text);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setNote(''), NOTE_MS);
  }).current;
  return [note, say] as const;
}

/** The meta line, with a way to pick out part of the text just after copying all of it. */
function Meta({ text, note, right, color, onSelect }: { text: string; note: string; right?: boolean; color: string; onSelect: () => void }) {
  const t = useTheme();
  return (
    <Text numberOfLines={1} style={[s.meta, { color }, right && { textAlign: 'right' }]}>
      {note || text}
      {note === COPIED && (
        <Text style={{ color: t.link }} onPress={onSelect}>
          {' · 选择部分文字'}
        </Text>
      )}
    </Text>
  );
}

const COPIED = '已复制全文';

export const Mine = memo(function Mine({ text, at, note, tone, copyable, onPress }: { text: string; at?: number; note?: string; tone?: 'pending' | 'queued' | 'error'; copyable?: boolean; onPress?: () => void }) {
  const t = useTheme();
  const [said, say] = useCopied();
  const [selecting, setSelecting] = useState(false);
  return (
    <View style={{ gap: 6 }}>
      <Pressable
        disabled={!copyable && !onPress}
        onPress={onPress}
        accessibilityRole={onPress ? 'button' : undefined}
        onLongPress={() => {
          Clipboard.setStringAsync(text).catch(() => undefined);
          say(COPIED);
        }}
        style={[s.mine, { backgroundColor: t.bubble }, tone === 'pending' && { opacity: 0.6 }, tone === 'queued' && { backgroundColor: t.waitBg, borderWidth: 1.5, borderStyle: 'dashed', borderColor: t.run }]}
      >
        <Text style={{ color: t.text, fontSize: 15, lineHeight: 22 }}>{text}</Text>
      </Pressable>
      <Meta text={note ?? (at ? clock(at) : '')} note={said} right color={tone === 'error' ? t.err : tone === 'queued' ? t.run : t.dim} onSelect={() => setSelecting(true)} />
      {selecting && <SelectSheet text={text} onClose={() => setSelecting(false)} />}
    </View>
  );
});

/** A stretch this long says so even with no command or edit to show: it read and thought. */
const LONG_S = 60;

/** 「干了 2 分钟 · 跑了 3 个命令 · 改了 chat.tsx +6 −2」, opening what it did call by call. */
export const Worked = memo(function Worked({ tools, onPress }: { tools: Tools; onPress?: () => void }) {
  const t = useTheme();
  const files = tools.files ?? [];
  const commands = tools.commands ?? 0;
  const took = tools.took ?? 0;
  // A stretch of reading has no command and no edit, but calls to look at all the same.
  const calls = !commands && !files.length ? (tools.calls ?? 0) : 0;
  if (!commands && !files.length && !calls && took < LONG_S) return null;
  const added = files.reduce((n, f) => n + f.added, 0);
  const removed = files.reduce((n, f) => n + f.removed, 0);
  const name = files.length === 1 ? files[0].path.split('/').pop() : `${files.length} 个文件`;
  return (
    <Pressable onPress={onPress} disabled={!onPress} style={[s.worked, { borderColor: t.sep }]}>
      <Text numberOfLines={1} style={{ color: t.dim, fontSize: 13, flex: 1 }}>
        {[took > 0 ? `干了 ${span(took)}` : '', commands > 0 ? `跑了 ${commands} 个命令` : '', calls > 0 ? `用了 ${calls} 个工具` : ''].filter(Boolean).join(' · ')}
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
});

export const Theirs = memo(function Theirs({ m, agent, onTools }: { m: Message; agent: string; onTools?: (seq: number) => void }) {
  const t = useTheme();
  const [said, say] = useCopied();
  const [selecting, setSelecting] = useState(false);
  return (
    <View style={{ gap: 8 }}>
      {m.tools && <Worked tools={m.tools} onPress={onTools ? () => onTools(m.seq) : undefined} />}
      <Pressable
        onLongPress={() => {
          Clipboard.setStringAsync(readable(m.text)).catch(() => undefined);
          say(COPIED);
        }}
      >
        <Markdown text={m.text} t={t} color={t.text} onNote={say} />
      </Pressable>
      <Meta text={[clock(m.at), agent].filter(Boolean).join(' · ')} note={said} color={t.dim} onSelect={() => setSelecting(true)} />
      {selecting && <SelectSheet text={readable(m.text)} onClose={() => setSelecting(false)} />}
    </View>
  );
});

const s = StyleSheet.create({
  mine: { alignSelf: 'flex-end', maxWidth: '80%', borderRadius: 20, paddingVertical: 11, paddingHorizontal: 15 },
  meta: { fontSize: 12 },
  worked: { flexDirection: 'row', alignItems: 'center', gap: 6, paddingVertical: 8, borderBottomWidth: 1 },
});
