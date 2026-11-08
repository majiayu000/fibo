// The composer: a big rounded card with the text on top and a row of small controls
// under it. The send key turns into stop while the Agent runs and nothing is typed.
// The + opens a sheet: take a photo or pick pictures (they wait as thumbnails above the text),
// and this conversation's permission. The row under the text keeps only +, the model and send.
import { type ComponentRef, useRef, useState } from 'react';
import { Image, Platform, Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { AttachSheet } from './attachsheet';
import { Icon } from './icons';
import { useBottomOverKeyboard } from './keyboard';
import { noOutline } from './kit';
import { modelName } from './parse';
import { MAX_PICTURES, type Picture, pick } from './pictures';
import { useTheme } from './theme';

export function Composer({
  agent,
  model,
  permission,
  running,
  canStop,
  offline,
  suggestion,
  onSend,
  onStop,
  onUseSuggestion,
  onModel,
  onPermission,
  canAttach,
  onError,
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
  offline: boolean;
  /** What the Agent offers as the next prompt: a tap puts it in the box, sending is still yours. */
  suggestion?: string | null;
  onSend: (text: string, pictures: Picture[]) => Promise<boolean>;
  /** The computer keeps pictures (`image`) and is online; without it there is no +. */
  canAttach?: boolean;
  onError?: (why: string) => void;
  onStop: () => void;
  onUseSuggestion?: () => void;
}) {
  const t = useTheme();
  const bottom = useBottomOverKeyboard();
  const [draft, setDraft] = useState('');
  const [pictures, setPictures] = useState<Picture[]>([]);
  const [choosing, setChoosing] = useState(false);
  const [picking, setPicking] = useState(false);
  const input = useRef<ComponentRef<typeof TextInput>>(null);
  const empty = !draft.trim() && !pictures.length;
  const stop = running && canStop && empty;
  async function go(text: string, with_: Picture[] = []) {
    if (!text.trim() && !with_.length) return;
    setDraft('');
    setPictures([]);
    if (!(await onSend(text.trim(), with_))) {
      setDraft(text);
      setPictures(with_);
    }
  }
  async function add(from: 'camera' | 'library') {
    setChoosing(false);
    setPicking(true);
    try {
      const got = await pick(from, MAX_PICTURES - pictures.length);
      if (got) setPictures((p) => [...p, ...got].slice(0, MAX_PICTURES));
    } catch (e) {
      onError?.((e as Error).message);
    } finally {
      setPicking(false);
    }
  }
  // The + opens pictures and this conversation's settings: shown when either is there.
  const sheetable = !!canAttach || pictures.length > 0 || !!permission || !!onPermission;
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
      {pictures.length > 0 && (
        <View style={s.pictures}>
          {pictures.map((p, i) => (
            <View key={p.uri}>
              <Image source={{ uri: p.uri }} style={[s.thumb, { backgroundColor: t.well }]} accessibilityLabel={`第 ${i + 1} 张图片`} />
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={`去掉第 ${i + 1} 张图片`}
                hitSlop={8}
                onPress={() => setPictures((all) => all.filter((x) => x !== p))}
                style={[s.unpick, { backgroundColor: t.text }]}
              >
                <Icon name="close" size={10} width={2.6} color={t.card} />
              </Pressable>
            </View>
          ))}
        </View>
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
        {sheetable && (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="添加图片和这个对话的设置"
            disabled={picking}
            onPress={() => setChoosing(true)}
            style={({ pressed }) => [s.round, { backgroundColor: t.well, opacity: picking ? 0.35 : pressed ? 0.6 : 1 }]}
          >
            <Icon name="plus" size={17} width={2.2} color={t.text} />
          </Pressable>
        )}
        {(!!model || !!agent) && (
          <Pressable
            disabled={!onModel}
            onPress={onModel}
            accessibilityRole={onModel ? 'button' : undefined}
            accessibilityLabel={`模型：${model ? modelName(model) : agent}`}
            style={({ pressed }) => [s.model, { backgroundColor: t.well }, pressed && { opacity: 0.6 }]}
          >
            <Text numberOfLines={1} style={{ color: t.text, fontSize: 13, flexShrink: 1 }}>
              {model ? modelName(model) : agent}
            </Text>
          </Pressable>
        )}
        <View style={{ flex: 1 }} />
        {running && canStop && !empty && (
          <Pressable accessibilityRole="button" accessibilityLabel="停下" onPress={onStop} style={({ pressed }) => [s.send, { borderWidth: 1, borderColor: t.line, backgroundColor: t.card }, pressed && { opacity: 0.6 }]}>
            <Icon name="stop" size={14} width={2.4} color={t.text} />
          </Pressable>
        )}
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={stop ? '停下' : '发送'}
          disabled={!stop && (empty || picking)}
          onPress={() => (stop ? onStop() : go(draft, pictures))}
          style={({ pressed }) => [
            s.send,
            { backgroundColor: t.act, opacity: !stop && (empty || picking) ? 0.35 : pressed ? 0.7 : 1 },
          ]}
        >
          <Icon name={stop ? 'stop' : 'up'} size={stop ? 14 : 16} width={2.4} color={t.onAct} />
        </Pressable>
      </View>
      {choosing && (
        <AttachSheet
          onClose={() => setChoosing(false)}
          pictures={canAttach ? (pictures.length >= MAX_PICTURES ? `已经选了 ${MAX_PICTURES} 张，发出去再加` : null) : pictures.length ? '电脑没连上，图片要连上后才能发' : undefined}
          onCamera={() => add('camera')}
          onLibrary={() => add('library')}
          permission={permission}
          onPermission={
            onPermission &&
            (() => {
              setChoosing(false);
              onPermission();
            })
          }
        />
      )}
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
  bar: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  round: { width: 36, height: 36, borderRadius: 18, alignItems: 'center', justifyContent: 'center' },
  model: { height: 36, borderRadius: 18, paddingHorizontal: 14, alignItems: 'center', justifyContent: 'center', flexShrink: 1 },
  pictures: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, paddingTop: 4, paddingRight: 4 },
  thumb: { width: 56, height: 56, borderRadius: 12 },
  unpick: { position: 'absolute', top: -5, right: -5, width: 18, height: 18, borderRadius: 9, alignItems: 'center', justifyContent: 'center' },
  send: { width: 40, height: 40, borderRadius: 20, alignItems: 'center', justifyContent: 'center' },
});
