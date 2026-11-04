// The composer: a big rounded card with the text on top and a row of small controls
// under it. The send key turns into stop while the Agent runs and nothing is typed.
import { useState } from 'react';
import { Platform, Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { Icon } from './icons';
import { noOutline, useBottom } from './kit';
import { useTheme } from './theme';

export function Composer({
  agent,
  running,
  canStop,
  canContinue,
  offline,
  onSend,
  onStop,
}: {
  agent: string;
  running: boolean;
  canStop: boolean;
  canContinue: boolean;
  offline: boolean;
  onSend: (text: string) => Promise<boolean>;
  onStop: () => void;
}) {
  const t = useTheme();
  const bottom = useBottom();
  const [draft, setDraft] = useState('');
  const stop = running && canStop && !draft.trim();
  async function go(text: string) {
    if (!text.trim()) return;
    setDraft('');
    if (!(await onSend(text.trim()))) setDraft(text);
  }
  return (
    <View style={[s.card, { borderColor: t.line, backgroundColor: t.card, marginBottom: bottom + 8, shadowColor: '#000' }]}>
      <TextInput
        value={draft}
        onChangeText={setDraft}
        placeholder={offline ? '电脑没连上，写的话会在重连后发送' : '回复，或者让它接着做…'}
        placeholderTextColor={t.dim}
        multiline
        numberOfLines={Platform.OS === 'web' ? 1 : undefined}
        style={[s.input, { color: t.text }, noOutline]}
        accessibilityLabel="给 Agent 发消息"
      />
      <View style={s.bar}>
        {!!agent && (
          <View style={[s.tag, { borderColor: t.line }]}>
            <Text style={{ color: t.text, fontSize: 12 }}>{agent}</Text>
          </View>
        )}
        {canContinue && (
          <Pressable onPress={() => go('继续')} style={({ pressed }) => [s.tag, { borderColor: t.line }, pressed && { opacity: 0.6 }]}>
            <Text style={{ color: t.text, fontSize: 12 }}>继续</Text>
          </Pressable>
        )}
        <View style={{ flex: 1 }} />
        <Pressable
          accessibilityLabel={stop ? '停下' : '发送'}
          disabled={!stop && !draft.trim()}
          onPress={() => (stop ? onStop() : go(draft))}
          style={({ pressed }) => [
            s.send,
            { backgroundColor: t.act, opacity: !stop && !draft.trim() ? 0.35 : pressed ? 0.7 : 1 },
          ]}
        >
          <Icon name={stop ? 'stop' : 'up'} size={stop ? 14 : 16} width={2.4} color={t.onAct} />
        </Pressable>
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  card: {
    marginHorizontal: 12,
    borderWidth: 1,
    borderRadius: 24,
    paddingTop: 12,
    paddingHorizontal: 12,
    paddingBottom: 10,
    gap: 10,
    shadowOpacity: 0.06,
    shadowRadius: 12,
    shadowOffset: { width: 0, height: 8 },
  },
  input: { fontSize: 15, minHeight: 24, maxHeight: 140, paddingHorizontal: 4, paddingVertical: 2 },
  bar: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  tag: { height: 30, borderRadius: 15, borderWidth: 1, paddingHorizontal: 10, alignItems: 'center', justifyContent: 'center' },
  send: { width: 34, height: 34, borderRadius: 17, alignItems: 'center', justifyContent: 'center' },
});
