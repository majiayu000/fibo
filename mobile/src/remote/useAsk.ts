// The approval a card shows: what it wants to run, as the machine read it off the screen.
import { useIsFocused } from 'expo-router';
import { useEffect, useState } from 'react';
import { type Ask, askFrom } from '../ui/parse';
import { type Card, remote } from './hub';

export function useAsk(machine: string, card: Card | undefined, online: boolean): Ask {
  const question = card?.approval ?? '';
  const [reply, setReply] = useState<Parameters<typeof askFrom>[0]>(null);
  // Every command asks in the same words, so the question alone cannot say a new one came.
  const moved = card?.updated;
  const terminal = card?.id;
  const which = `${machine}/${terminal}/${online}/${question}`;
  useEffect(() => setReply(null), [which]);
  // A screen another one covers asks nothing; it asks again when it comes back.
  const focused = useIsFocused();
  // Asked again as the card's log moves: what it said stays up until the new reply, or the row
  // would change height and lose its command under a finger each time. `answer` carries what
  // was shown, so the computer refuses it if the screen has moved on meanwhile.
  useEffect(() => {
    const r = focused && question && online && terminal ? remote(machine) : null;
    if (!r || !terminal) return;
    let live = true;
    r.request('ask', { terminal })
      .then((x) => live && setReply(x))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [machine, terminal, online, question, moved, focused]);
  return askFrom(reply, question);
}
