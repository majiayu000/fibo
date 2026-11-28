// The composer: a big rounded card with the text on top and a row of small controls
// under it. The send key turns into stop while the Agent runs and nothing is typed.
// The + opens a sheet: take a photo or pick pictures (they wait as thumbnails above the text),
// and this conversation's permission. The row under the text keeps only +, the model and send.
// Holding the empty box talks (holdtalk.tsx): letting go sends what was said.
import { type ComponentRef, useEffect, useRef, useState } from 'react';
import { Image, Platform, Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { AttachSheet } from './attachsheet';
import { useDictation } from './dictation';
import { useHold, VoiceGlow } from './holdtalk';
import { Icon } from './icons';
import { type How, STOP_AFTER_SEND_MS, stopCounts, whileBusy } from './busy';
import { useBottomOverKeyboard } from './keyboard';
import { noOutline } from './kit';
import { modelName } from './parse';
import { MAX_PICTURES, type Picture, pick } from './pictures';
import { useTheme } from './theme';
import { tr } from '../i18n';
import type { AgentControls } from '../remote/hub';
import type { ChatDraft } from '../store/chatDraft';

export function Composer({
  agent,
  model,
  permission,
  permissionFull,
  running,
  controls,
  canStop,
  offline,
  suggestion,
  onSend,
  onStop,
  onUseSuggestion,
  onModel,
  onPermission,
  canAttach,
  canSteer,
  onError,
  kept,
  onKeep,
  onTranscribe,
  placeholder,
  sendsEmpty,
  onResume,
}: {
  agent: string;
  /** What the computer says is on. */
  model?: string | null;
  permission?: string | null;
  permissionFull?: boolean | null;
  /** Each tag opens what can be picked; a computer that cannot change them leaves it a fact. */
  onModel?: () => void;
  onPermission?: () => void;
  running: boolean;
  controls?: AgentControls | null;
  canStop: boolean;
  offline: boolean;
  /** What the Agent offers as the next prompt: a tap puts it in the box, sending is still yours. */
  suggestion?: string | null;
  onSend: (text: string, pictures: Picture[], how?: How) => Promise<boolean>;
  /** It works and this computer and CLI can queue: a message sent then waits in the Agent's queue. */
  canSteer?: boolean;
  /** The computer keeps pictures (`image`) and is online; without it there is no +. */
  canAttach?: boolean;
  onError?: (why: string) => void;
  onStop: () => void;
  onUseSuggestion?: () => void;
  /** What this conversation's box held last time, read once; `onKeep` hears every change to it. */
  kept?: ChatDraft;
  onKeep?: (draft: ChatDraft) => void;
  /** The computer turns a recording into text, for a phone without its own recognizer. */
  onTranscribe?: (audio: string) => Promise<string>;
  /** Said in the empty box instead of the conversation's own words (the new-session page). */
  placeholder?: string;
  /** Sending nothing means something: picking up the last conversation needs no words. */
  sendsEmpty?: boolean;
  /** The + also offers to pick up the last conversation here. */
  onResume?: () => void;
}) {
  const t = useTheme();
  const bottom = useBottomOverKeyboard();
  const [draft, setDraft] = useState(kept?.text ?? '');
  const [pictures, setPictures] = useState<Picture[]>(kept?.pictures ?? []);
  // The box goes with the screen; what is written in it must not.
  useEffect(() => {
    onKeep?.({ text: draft, pictures });
  }, [draft, pictures, onKeep]);
  const [choosing, setChoosing] = useState(false);
  const [picking, setPicking] = useState(false);
  const input = useRef<ComponentRef<typeof TextInput>>(null);
  const dictation = useDictation(onError, onTranscribe);
  const empty = !draft.trim() && !pictures.length && !sendsEmpty;
  const stop = running && canStop && empty;
  const how = running && canSteer ? 'queue' : undefined;
  // A second tap before the box redraws empty would send the same words twice.
  const sending = useRef(false);
  // Just after a send, 停下 in the send key's place waits (STOP_AFTER_SEND_MS): a double tap must not stop the turn.
  const sentAt = useRef(0);
  const [justSent, setJustSent] = useState(false);
  const sentTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(sentTimer.current), []);
  const stopNow = () => {
    if (stopCounts(sentAt.current, Date.now())) onStop();
  };
  async function go(text: string, with_: Picture[] = []) {
    if ((!text.trim() && !with_.length && !sendsEmpty) || sending.current) return;
    sending.current = true;
    dictation.cancel();
    sentAt.current = Date.now();
    setJustSent(true);
    clearTimeout(sentTimer.current);
    sentTimer.current = setTimeout(() => setJustSent(false), STOP_AFTER_SEND_MS);
    setDraft('');
    setPictures([]);
    try {
      if (!(await onSend(text.trim(), with_, how))) {
        setDraft(text);
        setPictures(with_);
        // Left while it was on its way: this box is gone, the words are back for the next one.
        onKeep?.({ text, pictures: with_ });
      }
    } finally {
      sending.current = false;
    }
  }
  const hold = useHold(dictation, {
    said: (text) => void go(text, pictures),
    tap: () => input.current?.focus(),
    nothing: () => onError?.(tr('没听清，按住再说一次')),
  });
  const holdable = dictation.can && !draft;
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
  const sheetable = !!canAttach || pictures.length > 0 || !!permission || !!onPermission || !!onResume;
  return (
    <View style={[s.card, { borderColor: t.line, backgroundColor: t.card, marginBottom: bottom + 8, shadowColor: '#000' }, hold.shown && s.under]}>
      {hold.shown && <VoiceGlow hold={hold} dictation={dictation} letGo={tr('松手发送，上移取消')} style={{ left: -13, right: -13, bottom: -(bottom + 9) }} />}
      <View style={[s.body, hold.shown && { opacity: 0 }]}>
      {!!suggestion && !draft && (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={tr('填入建议：{s}', { s: suggestion })}
          onPress={() => {
            setDraft(suggestion);
            onUseSuggestion?.();
            input.current?.focus();
          }}
          style={({ pressed }) => [s.suggestion, { backgroundColor: t.well }, pressed && { opacity: 0.6 }]}
        >
          <Text style={{ color: t.dim, fontSize: 12 }}>{tr('建议')}</Text>
          <Text numberOfLines={2} style={{ color: t.text, fontSize: 14, lineHeight: 19, flexShrink: 1 }}>
            {suggestion}
          </Text>
        </Pressable>
      )}
      {pictures.length > 0 && (
        <View style={s.pictures}>
          {pictures.map((p, i) => (
            <View key={p.uri}>
              <Image source={{ uri: p.uri }} style={[s.thumb, { backgroundColor: t.well }]} accessibilityLabel={tr('第 {n} 张图片', { n: i + 1 })} />
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={tr('去掉第 {n} 张图片', { n: i + 1 })}
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
      <View>
        <TextInput
          ref={input}
          value={draft}
          onChangeText={setDraft}
          placeholder={placeholder ?? (offline ? tr('电脑没连上，写的话会在重连后发送') : running ? tr(whileBusy(controls, how).placeholder) : dictation.can ? tr('发消息，或按住说话') : tr('回复，或者让它接着做…'))}
          placeholderTextColor={t.dim}
          multiline
          numberOfLines={Platform.OS === 'web' ? 1 : undefined}
          style={[s.input, { color: t.text }, noOutline]}
          accessibilityLabel={tr('给 Agent 发消息')}
        />
        {/* Over the empty box only: a tap still types, a hold talks. */}
        {holdable && <View style={s.holdArea} {...hold.handlers} accessibilityLabel={tr('按住说话')} />}
      </View>
      <View style={s.bar}>
        {sheetable && (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={tr('添加图片和这个对话的设置')}
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
            accessibilityLabel={tr('模型：{m}', { m: model ? modelName(model) : agent })}
            style={({ pressed }) => [s.model, { backgroundColor: t.well }, pressed && { opacity: 0.6 }]}
          >
            <Text numberOfLines={1} style={{ color: t.text, fontSize: 13, flexShrink: 1 }}>
              {model ? modelName(model) : agent}
            </Text>
          </Pressable>
        )}
        <View style={{ flex: 1 }} />
        {running && canStop && !empty && (
          <Pressable accessibilityRole="button" accessibilityLabel={tr('停下')} onPress={onStop} style={({ pressed }) => [s.send, { borderWidth: 1, borderColor: t.line, backgroundColor: t.card }, pressed && { opacity: 0.6 }]}>
            <Icon name="stop" size={14} width={2.4} color={t.text} />
          </Pressable>
        )}
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={stop ? tr('停下') : tr('发送')}
          disabled={stop ? justSent : empty || picking}
          onPress={() => (stop ? stopNow() : go(draft, pictures))}
          style={({ pressed }) => [
            s.send,
            { backgroundColor: t.act, opacity: (stop ? justSent : empty || picking) ? 0.35 : pressed ? 0.7 : 1 },
          ]}
        >
          <Icon name={stop ? 'stop' : 'up'} size={stop ? 14 : 16} width={2.4} color={t.onAct} />
        </Pressable>
      </View>
      {choosing && (
        <AttachSheet
          onClose={() => setChoosing(false)}
          pictures={canAttach ? (pictures.length >= MAX_PICTURES ? tr('已经选了 {n} 张，发出去再加', { n: MAX_PICTURES }) : null) : pictures.length ? tr('电脑没连上，图片要连上后才能发') : undefined}
          onCamera={() => add('camera')}
          onLibrary={() => add('library')}
          permission={permission}
          permissionFull={permissionFull}
          onResume={
            onResume &&
            (() => {
              setChoosing(false);
              onResume();
            })
          }
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
  // The card steps aside while the glow is up, as Doubao's box does.
  under: { borderColor: 'transparent', backgroundColor: 'transparent', shadowOpacity: 0 },
  body: { gap: 10 },
  // Down past the row of keys would steal their taps; the box and a little around it.
  holdArea: { position: 'absolute', left: -12, right: -12, top: -12, bottom: -6 },
  suggestion: { flexDirection: 'row', alignItems: 'center', alignSelf: 'flex-start', maxWidth: '100%', gap: 8, borderRadius: 14, paddingHorizontal: 10, paddingVertical: 6 },
  input: { fontSize: 15, minHeight: 24, maxHeight: 140, paddingHorizontal: 4, paddingVertical: 2 },
  bar: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  round: { width: 36, height: 36, borderRadius: 18, alignItems: 'center', justifyContent: 'center' },
  model: { height: 36, borderRadius: 18, paddingHorizontal: 14, alignItems: 'center', justifyContent: 'center', flexShrink: 1 },
  pictures: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, paddingTop: 4, paddingRight: 4 },
  thumb: { width: 56, height: 56, borderRadius: 12 },
  unpick: { position: 'absolute', top: -5, right: -5, width: 18, height: 18, borderRadius: 9, alignItems: 'center', justifyContent: 'center' },
  send: { width: 44, height: 44, borderRadius: 22, alignItems: 'center', justifyContent: 'center' },
});
