// Talking through the computer: the phone records (its own AAC encoder), the computer turns it into
// text (`transcribe`, whisper.cpp there). For phones whose system lends apps no speech recognition
// (2026-11-28: an OPPO with none, a vivo whose recognizer hears nothing).
// On Android the recording is cut at each pause into pieces (adts.ts): a piece is read once more when
// the pause comes and its words are kept, and only the piece being spoken is read again every
// PARTIAL_MS, so each read is a few seconds of sound however long one talks.
import { useEffect, useRef, useState } from 'react';
import { type RecordingOptions, AudioQuality, IOSOutputFormat, requestRecordingPermissionsAsync, setAudioModeAsync, useAudioRecorder } from 'expo-audio';
import { File } from 'expo-file-system';
import { Platform } from 'react-native';
import { boundaryBefore, frameEnds, toBase64 } from './adts';
import { joinSpoken } from './spoken';
import { tr } from '../i18n';

/** Longer than anyone says to an Agent in one go; ~4 KB a second keeps it far under a protocol line. */
export const LONGEST_S = 90;
/** How often the piece being spoken is read again. */
export const PARTIAL_MS = 1000;
/** The first read once someone starts: a word or two in. */
const FIRST_MS = 600;
/** Quiet this long after speech ends a piece. */
const PAUSE_MS = 600;
/** A piece read this long is closed even without a pause, so no read grows past it. */
const LONGEST_PIECE_MS = 8000;
/** Louder than a quiet room (about -50 dBFS on 2026-11-28's vivo); -35 missed a softly spoken sentence. */
const VOICE_DB = -42;
/** The loudness is checked this often. */
const LISTEN_MS = 120;
/** Sound kept from before the first loud frame: the first syllable starts softer than the gate. */
const LEAD_BYTES = 1600;

// Speech needs no more: 16 kHz mono AAC at 32 kbit/s, what whisper reads anyway.
const VOICE: RecordingOptions = {
  extension: '.m4a',
  sampleRate: 16000,
  numberOfChannels: 1,
  bitRate: 32000,
  // The glow and the wave follow the voice.
  isMeteringEnabled: true,
  // ADTS, not MP4: an MP4 is only readable once its index is written at the end, ADTS frame by frame.
  android: { extension: '.aac', outputFormat: 'aac_adts', audioEncoder: 'aac' },
  ios: { outputFormat: IOSOutputFormat.MPEG4AAC, audioQuality: AudioQuality.MEDIUM },
  web: { mimeType: 'audio/webm', bitsPerSecond: 32000 },
};

export type VoiceMemo = {
  recording: boolean;
  /** Let go: the last piece on its way to the computer. */
  reading: boolean;
  start: () => Promise<void>;
  /** Everything said; '' when nobody spoke, null when it went wrong (already said through onError). */
  stop: () => Promise<string | null>;
  /** Slid away or left: drop the recording and any text still coming back. */
  cancel: () => void;
  /** How loud it is now, 0 to 1, asked by the glow: no state, no re-render. */
  level: () => number;
};

export function useVoiceMemo(
  transcribe: ((audio: string) => Promise<string>) | undefined,
  /** Everything heard so far, while recording. */
  onText: (said: string) => void,
  onError?: (why: string) => void,
): VoiceMemo {
  const recorder = useAudioRecorder(VOICE);
  const [recording, setRecording] = useState(false);
  const [reading, setReading] = useState(false);
  // Bumped by cancel and each start: a recording or answer from before it is dropped.
  const turn = useRef(0);
  const limit = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const stopRef = useRef<() => Promise<string | null>>(async () => null);
  const tick = useRef<ReturnType<typeof setInterval> | undefined>(undefined);
  // The words of every closed piece.
  const kept = useRef('');
  // Where the open piece starts in the file (a frame boundary), and where its first loud frame was.
  const pieceFrom = useRef(0);
  const voiceAt = useRef(0);
  // Nobody has spoken in this piece yet: whisper, given only room noise, makes up a sentence
  // (「我全要，我就不信拿不像的。」 on the vivo before the user started), so silence is never sent.
  const voiced = useRef(false);
  const heardAny = useRef(false);
  const lastVoice = useRef(0);
  const pieceOpened = useRef(0);
  const lastRead = useRef(0);
  // One read at a time; stop waits for it.
  const asking = useRef<Promise<void> | null>(null);

  function size(uri: string): number {
    try {
      return new File(uri).size ?? 0;
    } catch {
      return 0;
    }
  }

  /** Reads the open piece; `close` keeps its words and starts the next piece where this one ended. */
  async function readPiece(mine: number, close: boolean) {
    const uri = recorder.uri;
    if (!transcribe || !uri) return;
    const askedAt = Date.now();
    lastRead.current = askedAt;
    const b = await new File(uri).bytes();
    const ends = frameEnds(b, pieceFrom.current);
    const end = ends.at(-1);
    if (!end) return;
    const from = boundaryBefore(ends, pieceFrom.current, Math.max(pieceFrom.current, voiceAt.current - LEAD_BYTES));
    const said = (await transcribe(toBase64(b.subarray(from, end)))).trim();
    if (mine !== turn.current) return;
    if (close) {
      kept.current = joinSpoken(kept.current, said);
      pieceFrom.current = end;
      voiceAt.current = end;
      pieceOpened.current = Date.now();
      // Talking went on while this was read: the new piece already has a voice in it.
      voiced.current = lastVoice.current > askedAt;
      onText(kept.current);
    } else if (said) onText(joinSpoken(kept.current, said));
  }

  function ask(mine: number, close: boolean) {
    asking.current = readPiece(mine, close)
      .catch(() => undefined)
      .finally(() => {
        asking.current = null;
      });
  }

  function listen(mine: number) {
    const uri = recorder.uri;
    if (!uri || mine !== turn.current) return;
    const now = Date.now();
    if ((recorder.getStatus().metering ?? -160) > VOICE_DB) {
      if (!voiced.current) {
        voiced.current = true;
        voiceAt.current = size(uri);
        lastRead.current = now - PARTIAL_MS + FIRST_MS;
      }
      heardAny.current = true;
      lastVoice.current = now;
    }
    if (asking.current || !voiced.current) return;
    if (now - lastVoice.current >= PAUSE_MS || now - pieceOpened.current >= LONGEST_PIECE_MS) ask(mine, true);
    else if (now - lastRead.current >= PARTIAL_MS) ask(mine, false);
  }

  async function start() {
    if (!transcribe) return;
    const mine = ++turn.current;
    const got = await requestRecordingPermissionsAsync().catch(() => null);
    if (!got?.granted) return onError?.(tr('没有麦克风权限，到系统设置里给 Fibo 打开'));
    if (mine !== turn.current) return;
    ended.current = null;
    try {
      await setAudioModeAsync({ allowsRecording: true, playsInSilentMode: true });
      await recorder.prepareToRecordAsync();
      if (mine !== turn.current) return;
      recorder.record();
      kept.current = '';
      pieceFrom.current = 0;
      voiceAt.current = 0;
      voiced.current = false;
      heardAny.current = false;
      lastVoice.current = 0;
      pieceOpened.current = Date.now();
      setRecording(true);
      clearTimeout(limit.current);
      limit.current = setTimeout(() => void stopRef.current(), LONGEST_S * 1000);
      // An iPhone records MP4, unreadable until it ends: its own recognizer is used there anyway.
      if (Platform.OS === 'android') {
        clearInterval(tick.current);
        tick.current = setInterval(() => listen(mine), LISTEN_MS);
      }
    } catch (e) {
      onError?.(tr('录不了音：{why}', { why: (e as Error).message }));
    }
  }

  // What the last stop read: the 90-second limit stops it while the finger is still down, and letting go asks again.
  const ended = useRef<Promise<string | null> | null>(null);

  function stop(): Promise<string | null> {
    clearTimeout(limit.current);
    clearInterval(tick.current);
    tick.current = undefined;
    if (!recording || !transcribe) return ended.current ?? Promise.resolve(null);
    ended.current = finish();
    return ended.current;
  }

  async function finish(): Promise<string | null> {
    const mine = turn.current;
    const read = transcribe;
    if (!read) return null;
    setRecording(false);
    setReading(true);
    try {
      await recorder.stop();
      await asking.current;
      const uri = recorder.uri;
      if (!uri || mine !== turn.current) return null;
      const file = new File(uri);
      let said = kept.current;
      if (Platform.OS !== 'android') said = (await read(await file.base64())).trim();
      else if (voiced.current) {
        await readPiece(mine, true);
        said = kept.current;
      } else if (!heardAny.current) said = '';
      file.delete();
      return mine === turn.current ? said : null;
    } catch (e) {
      if (mine === turn.current) onError?.(tr('没转成文字：{why}', { why: (e as Error).message }));
      return null;
    } finally {
      if (mine === turn.current) setReading(false);
    }
  }
  stopRef.current = stop;

  function cancel() {
    turn.current++;
    ended.current = null;
    clearTimeout(limit.current);
    clearInterval(tick.current);
    tick.current = undefined;
    setReading(false);
    if (recording) {
      setRecording(false);
      void recorder.stop().catch(() => undefined);
    }
  }

  useEffect(
    () => () => {
      clearTimeout(limit.current);
      clearInterval(tick.current);
    },
    [],
  );

  // dBFS: room noise near -50, speech close to the phone near -15.
  const level = () => Math.min(1, Math.max(0, ((recorder.getStatus().metering ?? -160) + 50) / 38));

  return { recording, reading, start, stop, cancel, level };
}
