// Heard words come in jumps: a phrase from the phone's recognizer, two seconds' worth from the
// computer. Put into the box at once they look stuck, then leap (user, 2026-11-28: 「它不是一个一个
// 加载出来的，非常卡的感觉」). The typer walks the box toward what was heard a few characters a tick.
import { useEffect, useRef } from 'react';

export const TICK_MS = 30;
/** Never more behind than this many ticks: a long jump types faster. */
const CATCH_UP = 20;

/** The box one tick later: back to where both agree, then a step toward `to`. */
export function step(shown: string, to: string): string {
  const a = [...shown];
  const b = [...to];
  let same = 0;
  while (same < a.length && same < b.length && a[same] === b[same]) same++;
  // Words the recognizer took back go at once: typing them away again reads as a stutter.
  if (same < a.length) return b.slice(0, same).join('');
  const left = b.length - same;
  return b.slice(0, same + Math.max(1, Math.ceil(left / CATCH_UP))).join('');
}

export type Typer = {
  /** Listening starts: the box shows this now, whatever was typed by hand. */
  from: (text: string) => void;
  /** Type toward this text. */
  to: (text: string) => void;
  /** The user typed or sent: stop where it is. */
  stop: () => void;
};

export function useTyper(setText: (text: string) => void): Typer {
  const shown = useRef('');
  const target = useRef('');
  const tick = useRef<ReturnType<typeof setInterval> | undefined>(undefined);
  const set = useRef(setText);
  set.current = setText;

  const stop = () => {
    clearInterval(tick.current);
    tick.current = undefined;
  };
  useEffect(() => stop, []);

  return {
    from: (text) => {
      stop();
      shown.current = text;
      target.current = text;
    },
    to: (text) => {
      target.current = text;
      if (tick.current) return;
      tick.current = setInterval(() => {
        if (shown.current === target.current) return stop();
        shown.current = step(shown.current, target.current);
        set.current(shown.current);
      }, TICK_MS);
    },
    stop,
  };
}
