// Tap and scroll timing for lists that update underneath a finger.
import { useEffect, useMemo, useRef, useState } from 'react';
import type { GestureResponderEvent } from 'react-native';
import { HOLD_NONE, type Hold, type Settle, holdLeft, settleLeft, settleNext } from './inbox';

/**
 * Keeps a list still while a finger is down or it is scrolling. `held()` says so during render;
 * the component redraws once when it may move again. Touch events bubble without taking the
 * press away from what is under the finger, so these go on a View around the list.
 */
export function useHold() {
  const [, redraw] = useState(0);
  const hold = useRef<Hold>(HOLD_NONE);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  return useMemo(() => {
    const check = () => {
      if (timer.current) clearTimeout(timer.current);
      const left = holdLeft(hold.current, Date.now());
      timer.current = left > 0 ? setTimeout(check, left) : null;
      if (left <= 0) redraw((n) => n + 1);
    };
    const lift = (e: GestureResponderEvent) => {
      if ((e.nativeEvent.touches?.length ?? 0) > 0) return;
      hold.current = { ...hold.current, touching: false };
      check();
    };
    return {
      held: () => holdLeft(hold.current, Date.now()) > 0,
      touch: {
        onTouchStart: () => {
          hold.current = { ...hold.current, touching: true, touchedAt: Date.now() };
          check();
        },
        onTouchEnd: lift,
        onTouchCancel: lift,
      },
      onScroll: () => {
        hold.current = { ...hold.current, scrolledAt: Date.now() };
        check();
      },
    };
  }, []);
}

/**
 * Whether a tap on a card's buttons counts yet: false for a moment after `key` changes, so the
 * buttons look off and do nothing rather than answer something that just arrived under the
 * finger. `passes()` asks again at the tap itself, in case the screen has not redrawn.
 */
export function useSettled(key: string) {
  const [, redraw] = useState(0);
  const settle = useRef<Settle | null>(null);
  const now = Date.now();
  const s = settleNext(settle.current, key, now);
  settle.current = s;
  const left = settleLeft(s, now);
  useEffect(() => {
    if (left <= 0) return;
    const timer = setTimeout(() => redraw((n) => n + 1), left);
    return () => clearTimeout(timer);
  }, [left, s]);
  return { ready: left <= 0, passes: () => settleLeft(settle.current ?? s, Date.now()) <= 0 };
}
