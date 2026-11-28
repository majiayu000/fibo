// Talking into a box with the phone's own speech recognition (Android's SpeechRecognizer,
// iOS's SFSpeechRecognizer): the words land in the box as they are heard, sending is still yours.
// A phone without one records instead and the computer reads it (voicememo.ts), where the
// computer can; with neither there is no microphone and the keyboard's own voice key is left.
import { useEffect, useRef, useState } from 'react';
import { Animated, Easing, Platform, Pressable, StyleSheet, View } from 'react-native';
import { ExpoSpeechRecognitionModule, useSpeechRecognitionEvent } from 'expo-speech-recognition';
import { Icon } from './icons';
import { QUIET_ERRORS, joinSpoken } from './spoken';
import { useTyper } from './typer';
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

// Android 13+'s continuous mode hands the recognizer the app's own microphone stream
// (EXTRA_AUDIO_SOURCE); only Google's recognizer takes it. Others (2026-11-28: vivo's) hear
// nothing that way, so they listen themselves one phrase at a time and are started again.
/** A recognizer that has not said it is listening by now never will. */
const DEAF_MS = 3000;

const STREAMS = new Set(['com.google.android.tts', 'com.google.android.googlequicksearchbox', 'com.google.android.as']);

function continuous(): boolean {
  if (Platform.OS === 'ios') return true;
  if (Platform.OS !== 'android' || Number(Platform.Version) < 33) return false;
  try {
    return STREAMS.has(ExpoSpeechRecognitionModule.getDefaultRecognitionService().packageName);
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
  /** How loud it is now, 0 to 1: read by the microphone's halo while listening. */
  level: () => number;
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
  // The box as it was when recording started: each reading of what was said so far replaces the last.
  const memoBase = useRef('');
  const typer = useTyper(setText);
  const memo = useVoiceMemo(transcribe, (said, done) => {
    if (said.trim() || done) typer.to(joinSpoken(memoBase.current, said));
  }, onError);
  const volume = useRef(0);
  const textRef = useRef(text);
  textRef.current = text;
  const [listening, setListening] = useState(false);
  // The box as it was when listening started, plus every phrase heard to the end since.
  const base = useRef('');
  const mine = () => owner === id.current;
  // The microphone is green: the user has not stopped it yet.
  const wanted = useRef(false);
  // Armed by each start: a recognizer that never says it is listening (2026-11-28 vivo's, no
  // error and no words) is given up on, and the computer reads a recording instead.
  const silent = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const streams = useRef(false);

  useSpeechRecognitionEvent('start', () => {
    if (!mine()) return;
    clearTimeout(silent.current);
    setListening(true);
  });
  useSpeechRecognitionEvent('end', () => {
    if (!mine()) return;
    // One phrase heard, the microphone still green: listen for the next.
    if (wanted.current && !streams.current) return listen();
    owner = 0;
    setListening(false);
  });
  useSpeechRecognitionEvent('result', (e) => {
    if (!mine()) return;
    const heard = e.results[0]?.transcript ?? '';
    // Android ends each phrase and starts the next; iOS keeps one growing phrase.
    if (e.isFinal) {
      base.current = joinSpoken(base.current, heard);
      typer.to(base.current);
    } else typer.to(joinSpoken(base.current, heard));
  });
  // -2 to 10, below 0 inaudible.
  useSpeechRecognitionEvent('volumechange', (e) => {
    if (mine()) volume.current = Math.min(1, Math.max(0, e.value / 10));
  });
  useSpeechRecognitionEvent('error', (e) => {
    if (!mine()) return;
    // A pause between phrases: 'end' follows and starts the next.
    if (wanted.current && !streams.current && QUIET_ERRORS.has(e.error) && e.error !== 'aborted') return;
    wanted.current = false;
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
    typer.stop();
    wanted.current = false;
    clearTimeout(silent.current);
    if (!mine()) return;
    owner = 0;
    setListening(false);
    ExpoSpeechRecognitionModule.abort();
  };
  // Leaving the screen stops the microphone.
  useEffect(() => () => {
    clearTimeout(silent.current);
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
    typer.from(text);
    wanted.current = true;
    streams.current = continuous();
    setListening(true);
    listen();
  }

  function listen() {
    clearTimeout(silent.current);
    silent.current = setTimeout(() => {
      if (!mine()) return;
      wanted.current = false;
      owner = 0;
      setListening(false);
      ExpoSpeechRecognitionModule.abort();
      broken = true;
      setSystem(false);
      if (!transcribe) return onError?.(why('client', ''));
      memoBase.current = textRef.current;
      typer.from(textRef.current);
      void memo.start();
    }, DEAF_MS);
    ExpoSpeechRecognitionModule.start({
      lang: lang() === 'zh' ? 'zh-CN' : 'en-US',
      interimResults: true,
      // Keeps listening through pauses where the recognizer can; elsewhere 'end' starts it again.
      continuous: streams.current,
      addsPunctuation: true,
      volumeChangeEventOptions: { enabled: true, intervalMillis: 100 },
    });
  }

  if (!system && transcribe)
    return {
      can: true,
      listening: memo.recording,
      reading: memo.reading,
      toggle: () => {
        if (memo.recording) return void memo.stop();
        if (memo.reading) return;
        memoBase.current = textRef.current;
        typer.from(textRef.current);
        void memo.start();
      },
      level: memo.level,
      cancel,
    };
  return {
    can: system,
    listening,
    reading: false,
    toggle: () => {
      if (!listening) return void start();
      wanted.current = false;
      ExpoSpeechRecognitionModule.stop();
    },
    level: () => volume.current,
    cancel,
  };
}

/** The round microphone key: tap to talk, tap again to finish. Green while listening, with a halo that swells with the voice. */
export function MicButton({ dictation, disabled }: { dictation: Dictation; disabled?: boolean }) {
  const t = useTheme();
  if (!dictation.can) return null;
  const on = dictation.listening;
  return (
    <View style={s.round}>
      {on && <Halo level={dictation.level} color={t.allow} />}
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={on ? tr('说完了') : tr('说话输入')}
        accessibilityState={{ selected: on }}
        disabled={disabled || dictation.reading}
        onPress={dictation.toggle}
        style={({ pressed }) => [s.round, { backgroundColor: on ? t.allow : t.well, opacity: disabled || dictation.reading ? 0.35 : pressed ? 0.6 : 1 }]}
      >
        <Icon name="mic" size={17} width={2.1} color={on ? t.onAllow : t.text} />
      </Pressable>
    </View>
  );
}

/** Every HALO_MS the loudness is read and the halo eases toward it. */
const HALO_MS = 90;

function Halo({ level, color }: { level: () => number; color: string }) {
  const v = useRef(new Animated.Value(0)).current;
  const read = useRef(level);
  read.current = level;
  useEffect(() => {
    const tick = setInterval(() => {
      Animated.timing(v, { toValue: read.current(), duration: HALO_MS, easing: Easing.out(Easing.quad), useNativeDriver: true }).start();
    }, HALO_MS);
    return () => clearInterval(tick);
  }, [v]);
  return (
    <Animated.View
      pointerEvents="none"
      style={[
        StyleSheet.absoluteFill,
        s.halo,
        {
          backgroundColor: color,
          opacity: v.interpolate({ inputRange: [0, 1], outputRange: [0.18, 0.4] }),
          transform: [{ scale: v.interpolate({ inputRange: [0, 1], outputRange: [1.15, 1.75] }) }],
        },
      ]}
    />
  );
}

const s = StyleSheet.create({
  round: { width: 36, height: 36, borderRadius: 18, alignItems: 'center', justifyContent: 'center' },
  halo: { borderRadius: 18 },
});
