// Two bottom sheets: one asks for a sentence (要求修改, 拒绝并告诉它), one shows a
// terminal's last lines.
import { useEffect, useMemo, useRef, useState } from 'react';
import { KeyboardAvoidingView, Modal, Platform, Pressable, ScrollView, StyleSheet, Text, useWindowDimensions, TextInput, View } from 'react-native';
import { remote } from '../remote/hub';
import { CodeView, LINE } from './codeview';
import { copyWithNote } from './copy';
import { Btn, Notice, Wide, noOutline, useBottom } from './kit';
import { opensAtEnd, textRows } from './toolview';
import { useTheme } from './theme';

export function Frame({ visible, onClose, children }: { visible: boolean; onClose: () => void; children: React.ReactNode }) {
  const t = useTheme();
  const bottom = useBottom();
  return (
    <Modal visible={visible} transparent animationType="slide" onRequestClose={onClose}>
      <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined} style={{ flex: 1 }}>
        <Pressable style={s.scrim} onPress={onClose} />
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line, paddingBottom: bottom + 12 }]}>{children}</View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

/** A reply as plain selectable text: picking out part of it, after copying all of it by holding it. */
export function SelectSheet({ text, onClose }: { text: string; onClose: () => void }) {
  const t = useTheme();
  return (
    <Frame visible onClose={onClose}>
      <Text style={[s.title, { color: t.text }]}>选择文字</Text>
      <ScrollView style={{ maxHeight: 420 }}>
        <Text selectable style={{ color: t.text, fontSize: 15, lineHeight: 23 }}>
          {text}
        </Text>
      </ScrollView>
    </Frame>
  );
}

export function SayDialog({
  visible,
  title,
  placeholder,
  action,
  onSend,
  onClose,
}: {
  visible: boolean;
  title: string;
  placeholder: string;
  action: string;
  onSend: (text: string) => void;
  onClose: () => void;
}) {
  const t = useTheme();
  const [text, setText] = useState('');
  useEffect(() => {
    if (visible) setText('');
  }, [visible]);
  return (
    <Frame visible={visible} onClose={onClose}>
      <Text style={[s.title, { color: t.text }]}>{title}</Text>
      <TextInput
        autoFocus
        multiline
        value={text}
        onChangeText={setText}
        placeholder={placeholder}
        placeholderTextColor={t.dim}
        style={[s.input, { color: t.text, backgroundColor: t.well }, noOutline]}
      />
      <Wide label={action} disabled={!text.trim()} onPress={() => onSend(text.trim())} />
    </Frame>
  );
}

export function TerminalSheet({ visible, machine, terminal, onClose }: { visible: boolean; machine: string; terminal: string; onClose: () => void }) {
  const t = useTheme();
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const [loaded, setLoaded] = useState(false);
  const [note, setNote] = useState('');
  const closed = useRef(false);
  const { width } = useWindowDimensions();
  const rows = useMemo(() => textRows(text), [text]);
  useEffect(() => {
    closed.current = !visible;
    if (!visible) return;
    setLoaded(false);
    setNote('');
    const r = remote(machine);
    if (!r) return setError('电脑没连上');
    setError('');
    r.request('read', { terminal, lines: 40 })
      .then((x) => {
        setText(String(x.text ?? '').replace(/\s+$/, ''));
        setLoaded(true);
      })
      .catch((e) => setError((e as Error).message));
  }, [visible, machine, terminal]);
  return (
    <Frame visible={visible} onClose={onClose}>
      <View style={s.head}>
        <Text style={[s.title, { color: t.text }]}>终端最后几行</Text>
        <Btn label="复制" onPress={() => copyWithNote(text, '已复制', setNote, () => closed.current)} disabled={!text || !!error} />
      </View>
      {!!note && <Notice text={note} style={{ paddingVertical: 2 }} />}
      {text && !error ? (
        <View style={[s.screen, { backgroundColor: t.well }]}>
          <CodeView rows={rows} gap={noGap} footer={8} room={width - SHEET_PAD * 2} atEnd={opensAtEnd(rows.length, SCREEN_HEIGHT, LINE)} />
        </View>
      ) : (
        <Text style={{ color: t.dim, fontSize: 14, paddingVertical: 24, textAlign: 'center' }}>{error || (loaded ? '画面是空的' : '…')}</Text>
      )}
      <Wide label="关上" kind="grey" onPress={onClose} height={46} />
    </Frame>
  );
}

const SHEET_PAD = 16;
const SCREEN_HEIGHT = 440;
const noGap = () => <View />;

const s = StyleSheet.create({
  scrim: { flex: 1, backgroundColor: 'rgba(0,0,0,0.35)' },
  sheet: { borderTopLeftRadius: 24, borderTopRightRadius: 24, borderWidth: 1, padding: SHEET_PAD, gap: 12 },
  head: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12 },
  title: { fontSize: 17, fontWeight: '600' },
  input: { minHeight: 96, borderRadius: 14, padding: 12, fontSize: 15, textAlignVertical: 'top' },
  screen: { height: SCREEN_HEIGHT, borderRadius: 12, overflow: 'hidden' },
});
