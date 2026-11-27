// Talking into a box with the phone's own speech recognition (Android's SpeechRecognizer,
// iOS's SFSpeechRecognizer): the words land in the box as they are heard, sending is still yours.
// A phone without one records instead and the computer reads it (voicememo.ts), where the
// computer can; with neither there is no microphone and the keyboard's own voice key is left.
import { useEffect, useRef, useState } from 'react';
import { Platform, Pressable, StyleSheet } from 'react-native';
import { ExpoSpeechRecognitionModule, useSpeechRecognitionEvent } from 'expo-speech-recognition';
import { Icon } from './icons';
import { QUIET_ERRORS, joinSpoken } from './spoken';
import { useVoiceMemo } from './voicememo';
import { useTheme } from './theme';
import { lang, tr } from '../i18n';

// One recognizer for the whole phone: only the box that started it hears its events.
let owner = 0;
let next = 1;
// The recognizer said it cannot serve this app: no microphone again until the app restarts.
let broken = false;

function available(): boolean {
  // Every iPhone has a recognizer, but `isAvailable` is false while offline: there it says so on use instead.
  if (Platform.OS === 'ios') return true;
  if (broken) return false;
  try {
    if (!ExpoSpeechRecognitionModule.isRecognitionAvailable()) return false;
    // A phone can list a service another app keeps to itself (2026-11-28 OPPO: only Claude's, which
    // refuses others); with no default service set there is nothing this app can use.
    return Platform.OS !== 'android' || !!ExpoSpeechRecognitionModule.getDefaultRecognitionService().packageName;
  } catch {
    return false;
  }
}

/** The recognizer itself will not serve this app: asking again gives the same answer. */
const UNUSABLE = new Set(['service-not-allowed', 'client', 'bad-grammar', 'language-not-supported']);

function why(error: string, message: string): string {
  if (UNUSABLE.has(error)) return tr('这台手机的系统语音识别用不了，可以用输入法的语音键');
  if (error === 'not-allowed') return tr('没有麦克风权限，到系统设置里给 Fibo 打开');
  if (error === 'network') return tr('语音识别要联网，现在连不上');
  if (error === 'language-not-supported') return tr('系统的语音识别不支持这种语言');
  return tr('系统的语音识别用不了：{m}', { m: message || error });
}

export type Dictation = {
  /** The phone or the computer can read speech: without either there is no microphone to show. */
  can: boolean;
  listening: boolean;
  /** A recording on its way to the computer and back. */
  reading: boolean;
  toggle: () => void;
  /** Typing or sending: what is still being heard must not land in the box afterwards. */
  cancel: () => void;
};

export function useDictation(
  text: string,
  setText: (text: string) => void,
  onError?: (why: string) => void,
  /** The computer reads a recording (`transcribe`); for phones whose own recognizer will not do. */
  transcribe?: (audio: string) => Promise<string>,
): Dictation {
  const id = useRef(0);
  if (!id.current) id.current = next++;
  const [system, setSystem] = useState(available);
  const memo = useVoiceMemo(transcribe, (said) => setText(joinSpoken(textRef.current, said)), onError);
  const textRef = useRef(text);
  textRef.current = text;
  const [listening, setListening] = useState(false);
  // The box as it was when listening started, plus every phrase heard to the end since.
  const base = useRef('');
  const mine = () => owner === id.current;

  useSpeechRecognitionEvent('start', () => mine() && setListening(true));
  useSpeechRecognitionEvent('end', () => {
    if (!mine()) return;
    owner = 0;
    setListening(false);
  });
  useSpeechRecognitionEvent('result', (e) => {
    if (!mine()) return;
    const heard = e.results[0]?.transcript ?? '';
    // Android ends each phrase and starts the next; iOS keeps one growing phrase.
    if (e.isFinal) {
      base.current = joinSpoken(base.current, heard);
      setText(base.current);
    } else setText(joinSpoken(base.current, heard));
  });
  useSpeechRecognitionEvent('error', (e) => {
    if (!mine()) return;
    owner = 0;
    setListening(false);
    if (UNUSABLE.has(e.error)) {
      broken = true;
      setSystem(false);
      // The computer can still read it: the next tap records, nothing to say.
      if (transcribe) return;
    }
    if (!QUIET_ERRORS.has(e.error)) onError?.(why(e.error, e.message));
  });

  const cancel = () => {
    memo.cancel();
    if (!mine()) return;
    owner = 0;
    setListening(false);
    ExpoSpeechRecognitionModule.abort();
  };
  // Leaving the screen stops the microphone.
  useEffect(() => () => {
    if (owner === id.current) {
      owner = 0;
      ExpoSpeechRecognitionModule.abort();
    }
  }, []);

  async function start() {
    const got = await ExpoSpeechRecognitionModule.requestPermissionsAsync().catch(() => null);
    if (!got?.granted) {
      onError?.(why('not-allowed', ''));
      return;
    }
    if (owner) ExpoSpeechRecognitionModule.abort();
    owner = id.current;
    base.current = text;
    setListening(true);
    ExpoSpeechRecognitionModule.start({
      lang: lang() === 'zh' ? 'zh-CN' : 'en-US',
      interimResults: true,
      // Keeps listening through pauses; Android before 13 has no such mode and stops after one phrase.
      continuous: Platform.OS === 'ios' || (Platform.OS === 'android' && Number(Platform.Version) >= 33),
      addsPunctuation: true,
    });
  }

  if (!system && transcribe)
    return {
      can: true,
      listening: memo.recording,
      reading: memo.reading,
      toggle: () => void (memo.recording ? memo.stop() : memo.reading ? undefined : memo.start()),
      cancel,
    };
  return {
    can: system,
    listening,
    reading: false,
    toggle: () => (listening ? ExpoSpeechRecognitionModule.stop() : void start()),
    cancel,
  };
}

/** The round microphone key: tap to talk, tap again to finish. */
export function MicButton({ dictation, disabled }: { dictation: Dictation; disabled?: boolean }) {
  const t = useTheme();
  if (!dictation.can) return null;
  const on = dictation.listening;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={on ? tr('说完了') : tr('说话输入')}
      accessibilityState={{ selected: on }}
      disabled={disabled || dictation.reading}
      onPress={dictation.toggle}
      style={({ pressed }) => [s.round, { backgroundColor: on ? t.act : t.well, opacity: disabled || dictation.reading ? 0.35 : pressed ? 0.6 : 1 }]}
    >
      <Icon name="mic" size={17} width={2.1} color={on ? t.onAct : t.text} />
    </Pressable>
  );
}

const s = StyleSheet.create({
  round: { width: 36, height: 36, borderRadius: 18, alignItems: 'center', justifyContent: 'center' },
});
