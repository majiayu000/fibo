// What Claude offers as the next prompt, read off the computer's screen while it waits on you.
import { useEffect, useState } from 'react';
import { countOffer, countTurn } from '../store/suggestionCounts';
import { suggestionStats } from '../store/suggestions';
import { remote } from './hub';

const POLL_MS = 2000;

/** `turn` names the conversation's current turn; null asks for nothing. */
export function useSuggestion(machine: string, terminal: string, session: string | null, turn: string | null): string | null {
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    setText(null);
    if (!session || turn === null) return;
    const key = `${machine}/${session}/${turn}`;
    suggestionStats.set((s) => countTurn(s, key, Math.floor(Date.now() / 1000)));
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
  }, [machine, terminal, session, turn]);
  return text;
}
