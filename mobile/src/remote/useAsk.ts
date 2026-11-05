// The approval a card shows: what it wants to run, as the machine read it off the screen.
import { useEffect, useState } from 'react';
import { type Ask, askFrom } from '../ui/parse';
import { type Card, remote } from './hub';

export function useAsk(machine: string, card: Card | undefined, online: boolean): Ask {
  const question = card?.approval ?? '';
  const [reply, setReply] = useState<Parameters<typeof askFrom>[0]>(null);
  // Every command asks in the same words, so the question alone cannot say a new one came.
  const moved = card?.updated;
  const which = `${machine}/${card?.id}/${online}/${question}`;
  useEffect(() => setReply(null), [which]);
  // Asked again as the card's log moves: what it said stays up until the new reply, or the row
  // would change height and lose its command under a finger each time. `answer` carries what
  // was shown, so the computer refuses it if the screen has moved on meanwhile.
  useEffect(() => {
    const r = question && online && card ? remote(machine) : null;
    if (!r || !card) return;
    let live = true;
    r.request('ask', { terminal: card.id })
      .then((x) => live && setReply(x))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [which, moved]);
  return askFrom(reply, question);
}
