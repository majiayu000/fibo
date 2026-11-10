// Two bottom sheets: one asks for a sentence (要求修改, 拒绝并告诉它), one shows a
// terminal's last lines.
import { type ComponentRef, useEffect, useMemo, useRef, useState } from 'react';
import { Modal, Pressable, ScrollView, StyleSheet, Text, useWindowDimensions, TextInput, View } from 'react-native';
import { remote } from '../remote/hub';
import { CodeView, LINE } from './codeview';
import { copyWithNote } from './copy';
import { KeyboardSafe, useBottomOverKeyboard } from './keyboard';
import { Btn, Notice, Wide, noOutline, useBottom } from './kit';
import { opensAtEnd, textRows } from './toolview';
import { useTheme } from './theme';
import { tr } from '../i18n';

export function Frame({ visible, onClose, onShow, children }: { visible: boolean; onClose: () => void; onShow?: () => void; children: React.ReactNode }) {
  const t = useTheme();
  const bottom = useBottomOverKeyboard();
  return (
    <Modal visible={visible} transparent animationType="slide" onRequestClose={onClose} onShow={onShow}>
      {/* The dim is under the sheet too: only above it, the page showed undimmed past its
          rounded corners, a rectangle around a round one (2026-11-09). */}
      <KeyboardSafe style={s.dim}>
        <Pressable style={s.scrim} onPress={onClose} />
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line, paddingBottom: bottom + 12 }]}>{children}</View>
      </KeyboardSafe>
    </Modal>
  );
}

/** A reply as plain selectable text: picking out part of it, after copying all of it by holding it. */
export function SelectSheet({ text, onClose }: { text: string; onClose: () => void }) {
  const t = useTheme();
  return (
    <Frame visible onClose={onClose}>
      <Text style={[s.title, { color: t.text }]}>{tr('选择文字')}</Text>
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
  name,
}: {
  visible: boolean;
  title: string;
  placeholder: string;
  action: string;
  /** A name or path, not a sentence: no capital letter, no autocorrect, one line. */
  name?: boolean;
  onSend: (text: string) => void;
  onClose: () => void;
}) {
  const t = useTheme();
  const [text, setText] = useState('');
  const box = useRef<ComponentRef<typeof TextInput>>(null);
  useEffect(() => {
    if (visible) setText('');
  }, [visible]);
  return (
    // Inside a Modal, Android ignores autoFocus while the sheet slides in: ask once it is up.
    <Frame visible={visible} onClose={onClose} onShow={() => setTimeout(() => (box.current?.blur(), box.current?.focus()), 350)}>
      <Text style={[s.title, { color: t.text }]}>{title}</Text>
      <TextInput
        ref={box}
        autoFocus
        multiline={!name}
        autoCapitalize={name ? 'none' : undefined}
        autoCorrect={name ? false : undefined}
        value={text}
        onChangeText={setText}
        placeholder={placeholder}
        placeholderTextColor={t.dim}
        // Return sends, as the new task's box does; with nothing written it puts the keyboard away.
        returnKeyType="send"
        submitBehavior="blurAndSubmit"
        onSubmitEditing={() => text.trim() && onSend(text.trim())}
        style={[s.input, name && s.name, { color: t.text, backgroundColor: t.well }, noOutline]}
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
    if (!r) return setError(tr('电脑没连上'));
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
        <Text style={[s.title, { color: t.text }]}>{tr('终端最后几行')}</Text>
        <Btn label={tr('复制')} onPress={() => copyWithNote(text, tr('已复制'), setNote, () => closed.current)} disabled={!text || !!error} />
      </View>
      {!!note && <Notice text={note} style={{ paddingVertical: 2 }} />}
      {text && !error ? (
        <View style={[s.screen, { backgroundColor: t.well }]}>
          <CodeView rows={rows} gap={noGap} footer={8} room={width - SHEET_PAD * 2} atEnd={opensAtEnd(rows.length, SCREEN_HEIGHT, LINE)} />
        </View>
      ) : (
        <Text style={{ color: t.dim, fontSize: 14, paddingVertical: 24, textAlign: 'center' }}>{error || (loaded ? tr('画面是空的') : '…')}</Text>
      )}
      <Wide label={tr('关上')} kind="grey" onPress={onClose} height={46} />
    </Frame>
  );
}

const SHEET_PAD = 16;
const SCREEN_HEIGHT = 440;
const noGap = () => <View />;

const s = StyleSheet.create({
  dim: { backgroundColor: 'rgba(0,0,0,0.35)' },
  scrim: { flex: 1 },
  sheet: { borderTopLeftRadius: 24, borderTopRightRadius: 24, borderWidth: 1, padding: SHEET_PAD, gap: 12 },
  head: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12 },
  title: { fontSize: 17, fontWeight: '600' },
  input: { minHeight: 96, borderRadius: 14, padding: 12, fontSize: 15, textAlignVertical: 'top' },
  name: { minHeight: 48 },
  screen: { height: SCREEN_HEIGHT, borderRadius: 12, overflow: 'hidden' },
});
