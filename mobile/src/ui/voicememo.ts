// Talking into a box through the computer: the phone records (its own AAC encoder, an .m4a), the
// computer turns it into text (`transcribe`, whisper.cpp there) and the text lands in the box.
// For phones whose system lends apps no speech recognition (2026-11-28: an OPPO with none).
// While it records, what is recorded so far goes over every PARTIAL_MS, so the words show up
// as they are said, as WeChat's green microphone does; the whole recording is read once more
// at the end.
import { useEffect, useRef, useState } from 'react';
import { type RecordingOptions, AudioQuality, IOSOutputFormat, requestRecordingPermissionsAsync, setAudioModeAsync, useAudioRecorder } from 'expo-audio';
import { File } from 'expo-file-system';
import { Platform } from 'react-native';
import { tr } from '../i18n';

/** Longer than anyone says to an Agent in one go; ~4 KB a second keeps it far under a protocol line. */
export const LONGEST_S = 90;
/** How often the recording so far is read while talking. */
export const PARTIAL_MS = 1800;

// Speech needs no more: 16 kHz mono AAC at 32 kbit/s, what whisper reads anyway.
const VOICE: RecordingOptions = {
  extension: '.m4a',
  sampleRate: 16000,
  numberOfChannels: 1,
  bitRate: 32000,
  // The microphone's halo follows the voice.
  isMeteringEnabled: true,
  // ADTS, not MP4: an MP4 is only readable once its index is written at the end, ADTS frame by frame.
  android: { extension: '.aac', outputFormat: 'aac_adts', audioEncoder: 'aac' },
  ios: { outputFormat: IOSOutputFormat.MPEG4AAC, audioQuality: AudioQuality.MEDIUM },
  web: { mimeType: 'audio/webm', bitsPerSecond: 32000 },
};

export type VoiceMemo = {
  recording: boolean;
  /** Recorded and on its way to the computer. */
  reading: boolean;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  /** Typing, sending or leaving: drop the recording and any text still coming back. */
  cancel: () => void;
  /** How loud it is now, 0 to 1, asked by the halo while recording: no state, no re-render. */
  level: () => number;
};

export function useVoiceMemo(
  transcribe: ((audio: string) => Promise<string>) | undefined,
  /** Everything said so far; `done` once the recording ended and was read in full. */
  onText: (said: string, done: boolean) => void,
  onError?: (why: string) => void,
): VoiceMemo {
  const recorder = useAudioRecorder(VOICE);
  const [recording, setRecording] = useState(false);
  const [reading, setReading] = useState(false);
  // Bumped by cancel: a recording or answer from before it is dropped.
  const turn = useRef(0);
  const limit = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const stopRef = useRef<() => Promise<void>>(async () => {});
  const partial = useRef<ReturnType<typeof setInterval> | undefined>(undefined);
  // One partial read at a time: a slow computer is not sent more while it works.
  const asking = useRef(false);

  async function readSoFar(mine: number) {
    const uri = recorder.uri;
    if (!transcribe || !uri || asking.current) return;
    asking.current = true;
    try {
      const said = await transcribe(await new File(uri).base64());
      if (mine === turn.current && partial.current && said.trim()) onText(said, false);
    } catch {
      // The full read at the end says what went wrong.
    } finally {
      asking.current = false;
    }
  }

  async function start() {
    if (!transcribe) return;
    const got = await requestRecordingPermissionsAsync().catch(() => null);
    if (!got?.granted) return onError?.(tr('没有麦克风权限，到系统设置里给 Fibo 打开'));
    try {
      await setAudioModeAsync({ allowsRecording: true, playsInSilentMode: true });
      await recorder.prepareToRecordAsync();
      recorder.record();
      setRecording(true);
      clearTimeout(limit.current);
      limit.current = setTimeout(() => void stopRef.current(), LONGEST_S * 1000);
      // An iPhone records MP4, unreadable until it ends: its own recognizer is used there anyway.
      if (Platform.OS === 'android') {
        const mine = turn.current;
        clearInterval(partial.current);
        partial.current = setInterval(() => void readSoFar(mine), PARTIAL_MS);
      }
    } catch (e) {
      onError?.(tr('录不了音：{why}', { why: (e as Error).message }));
    }
  }

  async function stop() {
    clearTimeout(limit.current);
    clearInterval(partial.current);
    partial.current = undefined;
    if (!recording || !transcribe) return;
    const mine = turn.current;
    setRecording(false);
    try {
      await recorder.stop();
      const uri = recorder.uri;
      if (!uri || mine !== turn.current) return;
      setReading(true);
      const file = new File(uri);
      const audio = await file.base64();
      file.delete();
      const said = await transcribe(audio);
      if (mine === turn.current) onText(said, true);
    } catch (e) {
      if (mine === turn.current) onError?.(tr('没转成文字：{why}', { why: (e as Error).message }));
    } finally {
      if (mine === turn.current) setReading(false);
    }
  }
  stopRef.current = stop;

  function cancel() {
    turn.current++;
    clearTimeout(limit.current);
    clearInterval(partial.current);
    partial.current = undefined;
    setReading(false);
    if (recording) {
      setRecording(false);
      void recorder.stop().catch(() => undefined);
    }
  }

  useEffect(
    () => () => {
      clearTimeout(limit.current);
      clearInterval(partial.current);
    },
    [],
  );

  // dBFS: room noise near -50, speech close to the phone near -15.
  const level = () => Math.min(1, Math.max(0, ((recorder.getStatus().metering ?? -160) + 50) / 38));

  return { recording, reading, start, stop, cancel, level };
}
