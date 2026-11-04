// Two bottom sheets: one asks for a sentence (要求修改, 拒绝并告诉它), one shows a
// terminal's last lines.
import { useEffect, useState } from 'react';
import { KeyboardAvoidingView, Modal, Platform, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { remote } from '../remote/hub';
import { Wide, mono, noOutline, useBottom } from './kit';
import { useTheme } from './theme';

function Frame({ visible, onClose, children }: { visible: boolean; onClose: () => void; children: React.ReactNode }) {
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
  useEffect(() => {
    if (!visible) return;
    setLoaded(false);
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
      <Text style={[s.title, { color: t.text }]}>终端最后几行</Text>
      <ScrollView style={{ maxHeight: 420 }} horizontal={false}>
        <Text selectable style={[s.screen, { color: t.text, backgroundColor: t.well }]}>
          {error || (loaded ? text || '画面是空的' : '…')}
        </Text>
      </ScrollView>
      <Wide label="关上" kind="grey" onPress={onClose} height={46} />
    </Frame>
  );
}

const s = StyleSheet.create({
  scrim: { flex: 1, backgroundColor: 'rgba(0,0,0,0.35)' },
  sheet: { borderTopLeftRadius: 24, borderTopRightRadius: 24, borderWidth: 1, padding: 16, gap: 12 },
  title: { fontSize: 17, fontWeight: '600' },
  input: { minHeight: 96, borderRadius: 14, padding: 12, fontSize: 15, textAlignVertical: 'top' },
  screen: { fontFamily: mono, fontSize: 12, lineHeight: 17, padding: 10, borderRadius: 12 },
});
