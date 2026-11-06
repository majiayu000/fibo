// What Claude offers as the next prompt, read off the computer's screen while it waits on you.
import { useIsFocused } from 'expo-router';
import { useEffect, useState } from 'react';
import { countOffer, countTurn } from '../store/suggestionCounts';
import { suggestionStats } from '../store/suggestions';
import { remote } from './hub';

const POLL_MS = 2000;

/** `turn` names the conversation's current turn; null asks for nothing. */
export function useSuggestion(machine: string, terminal: string, session: string | null, turn: string | null): string | null {
  const [text, setText] = useState<string | null>(null);
  const focused = useIsFocused();
  useEffect(() => {
    setText(null);
    if (!session || turn === null) return;
    const key = `${machine}/${session}/${turn}`;
    suggestionStats.set((s) => countTurn(s, key, Math.floor(Date.now() / 1000)));
  }, [machine, session, turn]);
  // Asked only while this screen shows: a tool's page opened over it costs nothing.
  useEffect(() => {
    if (!session || turn === null || !focused) return;
    const key = `${machine}/${session}/${turn}`;
    let live = true;
    const ask = async () => {
      try {
        const got = await remote(machine)?.request('suggestion', { terminal });
        if (!live) return;
        const offered = got?.session === session ? (got.text ?? null) : null;
        setText(offered);
        if (offered) suggestionStats.set((s) => countOffer(s, key));
      } catch {
        if (live) setText(null);
      }
    };
    ask();
    const timer = setInterval(ask, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [machine, terminal, session, turn, focused]);
  return text;
}
