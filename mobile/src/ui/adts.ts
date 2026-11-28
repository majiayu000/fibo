// An Android recording is AAC in ADTS: every frame carries its own 7-byte header, so any run of
// whole frames is a file of its own that ffmpeg reads. That lets the phone send the computer only
// the part said since the last pause instead of the whole recording again (user, 2026-11-28:
// 「语音输入不是流式吗？」 — re-reading everything from the start fell further behind the longer one talked).

/** Where each whole frame after `from` ends; `from` must itself be a frame boundary. A frame still being written is left out. */
export function frameEnds(b: Uint8Array, from = 0): number[] {
  const ends: number[] = [];
  let at = from;
  while (at + 7 <= b.length && b[at] === 0xff && (b[at + 1] & 0xf0) === 0xf0) {
    const len = ((b[at + 3] & 0x03) << 11) | (b[at + 4] << 3) | ((b[at + 5] & 0xe0) >> 5);
    if (len < 7 || at + len > b.length) break;
    at += len;
    ends.push(at);
  }
  return ends;
}

/** The last frame boundary at or before `at`, counting `from` as one. */
export function boundaryBefore(ends: number[], from: number, at: number): number {
  let best = from;
  for (const e of ends) {
    if (e > at) break;
    best = e;
  }
  return best;
}

export function toBase64(b: Uint8Array): string {
  let s = '';
  for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000));
  return btoa(s);
}
