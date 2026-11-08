// The composer: a big rounded card with the text on top and a row of small controls
// under it. The send key turns into stop while the Agent runs and nothing is typed.
import { type ComponentRef, useRef, useState } from 'react';
import { Platform, Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { Icon } from './icons';
import { noOutline, useBottom } from './kit';
import { isFull } from './modes';
import { modelName } from './parse';
import { useTheme } from './theme';

export function Composer({
  agent,
  model,
  permission,
  running,
  canStop,
  canContinue,
  offline,
  suggestion,
  onSend,
  onStop,
  onUseSuggestion,
  onModel,
  onPermission,
}: {
  agent: string;
  /** What the computer says is on. */
  model?: string | null;
  permission?: string | null;
  /** Each tag opens what can be picked; a computer that cannot change them leaves it a fact. */
  onModel?: () => void;
  onPermission?: () => void;
  running: boolean;
  canStop: boolean;
  canContinue: boolean;
  offline: boolean;
  /** What the Agent offers as the next prompt: a tap puts it in the box, sending is still yours. */
  suggestion?: string | null;
  onSend: (text: string) => Promise<boolean>;
  onStop: () => void;
  onUseSuggestion?: () => void;
}) {
  const t = useTheme();
  const bottom = useBottom();
  const [draft, setDraft] = useState('');
  const input = useRef<ComponentRef<typeof TextInput>>(null);
  const stop = running && canStop && !draft.trim();
  async function go(text: string) {
    if (!text.trim()) return;
    setDraft('');
    if (!(await onSend(text.trim()))) setDraft(text);
  }
  return (
    <View style={[s.card, { borderColor: t.line, backgroundColor: t.card, marginBottom: bottom + 8, shadowColor: '#000' }]}>
      {!!suggestion && !draft && (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={`填入建议：${suggestion}`}
          onPress={() => {
            setDraft(suggestion);
            onUseSuggestion?.();
            input.current?.focus();
          }}
          style={({ pressed }) => [s.suggestion, { backgroundColor: t.well }, pressed && { opacity: 0.6 }]}
        >
          <Text style={{ color: t.dim, fontSize: 12 }}>建议</Text>
          <Text numberOfLines={2} style={{ color: t.text, fontSize: 14, lineHeight: 19, flexShrink: 1 }}>
            {suggestion}
          </Text>
        </Pressable>
      )}
      <TextInput
        ref={input}
        value={draft}
        onChangeText={setDraft}
        placeholder={offline ? '电脑没连上，写的话会在重连后发送' : running ? '它还在做事：现在发的，做完手头这一步就会看到' : '回复，或者让它接着做…'}
        placeholderTextColor={t.dim}
        multiline
        numberOfLines={Platform.OS === 'web' ? 1 : undefined}
        style={[s.input, { color: t.text }, noOutline]}
        accessibilityLabel="给 Agent 发消息"
      />
      <View style={s.bar}>
        {(!!permission || !!onPermission) && (
          <Pressable
            disabled={!onPermission}
            onPress={onPermission}
            accessibilityRole={onPermission ? 'button' : undefined}
            accessibilityLabel={`权限：${permission ?? '还没读到'}`}
            style={({ pressed }) => [s.tag, s.fact, { borderColor: isFull(permission) ? t.err : t.line }, pressed && { opacity: 0.6 }]}
          >
            <Icon name="shield" size={13} width={2} color={isFull(permission) ? t.err : t.text} />
            <Text numberOfLines={1} style={{ color: isFull(permission) ? t.err : t.text, fontSize: 12, flexShrink: 1 }}>
              {permission ?? '权限'}
            </Text>
          </Pressable>
        )}
        {(!!model || !!agent) && (
          <Pressable
            disabled={!onModel}
            onPress={onModel}
            accessibilityRole={onModel ? 'button' : undefined}
            accessibilityLabel={`模型：${model ? modelName(model) : agent}`}
            style={({ pressed }) => [s.tag, s.fact, { borderColor: t.line }, pressed && { opacity: 0.6 }]}
          >
            <Text numberOfLines={1} style={{ color: t.text, fontSize: 12, flexShrink: 1 }}>
              {model ? modelName(model) : agent}
            </Text>
          </Pressable>
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
  suggestion: { flexDirection: 'row', alignItems: 'center', alignSelf: 'flex-start', maxWidth: '100%', gap: 8, borderRadius: 14, paddingHorizontal: 10, paddingVertical: 6 },
  input: { fontSize: 15, minHeight: 24, maxHeight: 140, paddingHorizontal: 4, paddingVertical: 2 },
  bar: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  tag: { height: 30, borderRadius: 15, borderWidth: 1, paddingHorizontal: 10, alignItems: 'center', justifyContent: 'center' },
  fact: { flexDirection: 'row', gap: 5, flexShrink: 1 },
  send: { width: 34, height: 34, borderRadius: 17, alignItems: 'center', justifyContent: 'center' },
});
