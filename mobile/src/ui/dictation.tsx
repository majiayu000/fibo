// Hold to talk (holdtalk.tsx draws it): what is heard shows while the finger is down, letting go
// hands over everything said. The phone's own speech recognition (Android's SpeechRecognizer,
// iOS's SFSpeechRecognizer) where it serves apps; elsewhere the phone records and the computer
// reads it (voicememo.ts); with neither there is nothing to hold and the keyboard's voice key is left.
import { useEffect, useRef, useState } from 'react';
import { Platform } from 'react-native';
import { ExpoSpeechRecognitionModule, useSpeechRecognitionEvent } from 'expo-speech-recognition';
import { QUIET_ERRORS, joinSpoken } from './spoken';
import { useTyper } from './typer';
import { remembered } from '../store/kv';
import { useVoiceMemo } from './voicememo';
import { lang, tr } from '../i18n';

// One recognizer for the whole phone: only the box that started it hears its events.
let owner = 0;
let next = 1;
// The recognizer said it cannot serve this app, or never listened: not asked again until the app restarts.
let broken = false;
// And kept across restarts for the service that failed: each first hold after a restart spent 3 s
// on vivo's deaf recognizer and the words said meanwhile were lost (2026-11-28, 「帮我把首页的标题改成蓝色」
// gone from the front of a sentence). Another default service set later is tried afresh.
const deaf = remembered('dictation.deaf', { service: '' });

function service(): string {
  try {
    return Platform.OS === 'android' ? (ExpoSpeechRecognitionModule.getDefaultRecognitionService().packageName ?? '') : '';
  } catch {
    return '';
  }
}

function giveUp() {
  broken = true;
  const failed = service();
  if (failed) deaf.set({ service: failed });
}

function available(): boolean {
  // Every iPhone has a recognizer, but `isAvailable` is false while offline: there it says so on use instead.
  if (Platform.OS === 'ios') return true;
  if (broken) return false;
  if (deaf.get().service && deaf.get().service === service()) return false;
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
  /** The phone or the computer can read speech: without either there is nothing to hold. */
  can: boolean;
  /** The finger is down and the microphone on. */
  listening: boolean;
  /** Let go: the last words on their way back. */
  reading: boolean;
  /** What has been heard so far, typed in a few characters at a time. */
  heard: string;
  begin: () => void;
  /** Let go: everything said once it is all read; '' when nothing was heard or it went wrong. */
  finish: () => Promise<string>;
  /** Slid away or left: nothing heard is handed over. */
  cancel: () => void;
  /** How loud it is now, 0 to 1. */
  level: () => number;
};

/** After stop() a recognizer gives its last words and 'end'; one that never does is not waited on longer. */
const LAST_WORDS_MS = 4000;

export function useDictation(
  onError?: (why: string) => void,
  /** The computer reads a recording (`transcribe`); for phones whose own recognizer will not do. */
  transcribe?: (audio: string) => Promise<string>,
): Dictation {
  const id = useRef(0);
  if (!id.current) id.current = next++;
  const [system, setSystem] = useState(available);
  const [heard, setHeard] = useState('');
  const typer = useTyper(setHeard);
  // The computer's reading: each one is everything said so far.
  const memo = useVoiceMemo(transcribe, (said) => typer.to(said), onError);
  // This hold went to the computer: the recognizer never said it was listening.
  const viaMemo = useRef(false);
  const volume = useRef(0);
  const [listening, setListening] = useState(false);
  const [waiting, setWaiting] = useState(false);
  // Every phrase heard to the end, and that plus the phrase being heard.
  const base = useRef('');
  const all = useRef('');
  const mine = () => owner === id.current;
  // The finger is still down.
  const wanted = useRef(false);
  // Let go: resolved with everything heard once the recognizer ends.
  const done = useRef<((said: string) => void) | null>(null);
  const lastWords = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // Armed by each start: a recognizer that never says it is listening (2026-11-28 vivo's, no
  // error and no words) is given up on, and the computer reads a recording instead.
  const silent = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const streams = useRef(false);

  function settle(said: string) {
    clearTimeout(lastWords.current);
    const resolve = done.current;
    done.current = null;
    setWaiting(false);
    resolve?.(said);
  }

  useSpeechRecognitionEvent('start', () => {
    if (!mine()) return;
    clearTimeout(silent.current);
    setListening(true);
  });
  useSpeechRecognitionEvent('end', () => {
    if (!mine()) return;
    // One phrase heard, the finger still down: listen for the next.
    if (wanted.current && !streams.current) return listen();
    owner = 0;
    setListening(false);
    settle(all.current);
  });
  useSpeechRecognitionEvent('result', (e) => {
    if (!mine()) return;
    const said = e.results[0]?.transcript ?? '';
    // Android ends each phrase and starts the next; iOS keeps one growing phrase.
    if (e.isFinal) base.current = joinSpoken(base.current, said);
    all.current = e.isFinal ? base.current : joinSpoken(base.current, said);
    typer.to(all.current);
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
    settle(all.current);
    if (UNUSABLE.has(e.error)) {
      giveUp();
      setSystem(false);
      // The computer can still read it: the next hold records, nothing to say.
      if (transcribe) return;
    }
    if (!QUIET_ERRORS.has(e.error)) onError?.(why(e.error, e.message));
  });

  const cancel = () => {
    memo.cancel();
    typer.stop();
    setHeard('');
    wanted.current = false;
    viaMemo.current = false;
    clearTimeout(silent.current);
    settle('');
    if (!mine()) return;
    owner = 0;
    setListening(false);
    ExpoSpeechRecognitionModule.abort();
  };
  // Leaving the screen stops the microphone.
  useEffect(() => () => {
    clearTimeout(silent.current);
    clearTimeout(lastWords.current);
    if (owner === id.current) {
      owner = 0;
      ExpoSpeechRecognitionModule.abort();
    }
  }, []);

  function toMemo() {
    viaMemo.current = true;
    typer.from('');
    setHeard('');
    void memo.start();
  }

  async function begin() {
    typer.from('');
    setHeard('');
    base.current = '';
    all.current = '';
    wanted.current = true;
    // Read again: what an earlier run learned is loaded after the first screen asked.
    if (!system || !available()) return toMemo();
    viaMemo.current = false;
    const got = await ExpoSpeechRecognitionModule.requestPermissionsAsync().catch(() => null);
    if (!got?.granted) {
      wanted.current = false;
      onError?.(why('not-allowed', ''));
      return;
    }
    // Let go while the permission was asked.
    if (!wanted.current) return;
    if (owner) ExpoSpeechRecognitionModule.abort();
    owner = id.current;
    streams.current = continuous();
    setListening(true);
    listen();
  }

  function listen() {
    clearTimeout(silent.current);
    silent.current = setTimeout(() => {
      if (!mine()) return;
      owner = 0;
      setListening(false);
      ExpoSpeechRecognitionModule.abort();
      giveUp();
      setSystem(false);
      if (!transcribe) {
        wanted.current = false;
        return onError?.(why('client', ''));
      }
      // The finger is still down: the rest of this hold is recorded.
      if (wanted.current) toMemo();
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

  async function finish(): Promise<string> {
    wanted.current = false;
    if (viaMemo.current) {
      viaMemo.current = false;
      const said = await memo.stop();
      // Let go before the recorder had started: it must not start after all.
      if (said === null) memo.cancel();
      typer.stop();
      return said ?? '';
    }
    clearTimeout(silent.current);
    if (!mine()) return all.current;
    setWaiting(true);
    const said = new Promise<string>((resolve) => (done.current = resolve));
    lastWords.current = setTimeout(() => {
      if (mine()) {
        owner = 0;
        setListening(false);
        ExpoSpeechRecognitionModule.abort();
      }
      settle(all.current);
    }, LAST_WORDS_MS);
    ExpoSpeechRecognitionModule.stop();
    const got = await said;
    typer.stop();
    return got;
  }

  const memoOn = viaMemo.current || (!system && !!transcribe);
  return {
    can: system || !!transcribe,
    listening: memoOn ? memo.recording : listening && !waiting,
    reading: memoOn ? memo.reading : waiting,
    heard,
    begin: () => void begin(),
    finish,
    cancel,
    level: memoOn ? memo.level : () => volume.current,
  };
}
