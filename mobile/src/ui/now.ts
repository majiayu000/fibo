// The clock for durations on screen: one timer however many rows show one, and only while
// some do, so a list of idle conversations never redraws for it.
import { useSyncExternalStore } from 'react';

const EVERY_MS = 30_000;
const listeners = new Set<() => void>();
let now = Date.now() / 1000;
let timer: ReturnType<typeof setInterval> | undefined;

function subscribe(l: () => void) {
  listeners.add(l);
  now = Date.now() / 1000;
  timer ??= setInterval(() => {
    now = Date.now() / 1000;
    listeners.forEach((x) => x());
  }, EVERY_MS);
  return () => {
    listeners.delete(l);
    if (!listeners.size && timer) {
      clearInterval(timer);
      timer = undefined;
    }
  };
}

export const useNow = () => useSyncExternalStore(subscribe, () => now);
