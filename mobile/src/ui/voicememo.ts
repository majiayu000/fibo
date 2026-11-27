// Talking into a box through the computer: the phone records (its own AAC encoder, an .m4a), the
// computer turns it into text (`transcribe`, whisper.cpp there) and the text lands in the box.
// For phones whose system lends apps no speech recognition (2026-11-28: an OPPO with none).
import { useEffect, useRef, useState } from 'react';
import { type RecordingOptions, AudioQuality, IOSOutputFormat, requestRecordingPermissionsAsync, setAudioModeAsync, useAudioRecorder } from 'expo-audio';
import { File } from 'expo-file-system';
import { tr } from '../i18n';

/** Longer than anyone says to an Agent in one go; ~4 KB a second keeps it far under a protocol line. */
export const LONGEST_S = 90;

// Speech needs no more: 16 kHz mono AAC at 32 kbit/s, what whisper reads anyway.
const VOICE: RecordingOptions = {
  extension: '.m4a',
  sampleRate: 16000,
  numberOfChannels: 1,
  bitRate: 32000,
  android: { outputFormat: 'mpeg4', audioEncoder: 'aac' },
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
};

export function useVoiceMemo(
  transcribe: ((audio: string) => Promise<string>) | undefined,
  onText: (said: string) => void,
  onError?: (why: string) => void,
): VoiceMemo {
  const recorder = useAudioRecorder(VOICE);
  const [recording, setRecording] = useState(false);
  const [reading, setReading] = useState(false);
  // Bumped by cancel: a recording or answer from before it is dropped.
  const turn = useRef(0);
  const limit = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const stopRef = useRef<() => Promise<void>>(async () => {});

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
    } catch (e) {
      onError?.(tr('录不了音：{why}', { why: (e as Error).message }));
    }
  }

  async function stop() {
    clearTimeout(limit.current);
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
      if (mine === turn.current && said.trim()) onText(said);
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
    setReading(false);
    if (recording) {
      setRecording(false);
      void recorder.stop().catch(() => undefined);
    }
  }

  useEffect(() => () => clearTimeout(limit.current), []);

  return { recording, reading, start, stop, cancel };
}
