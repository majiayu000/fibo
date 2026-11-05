// The approval a card shows: what it wants to run, as the machine read it off the screen.
import { useEffect, useState } from 'react';
import { type Ask, askFrom } from '../ui/parse';
import { type Card, remote } from './hub';

export function useAsk(machine: string, card: Card | undefined, online: boolean): Ask {
  const question = card?.approval ?? '';
  const [reply, setReply] = useState<Parameters<typeof askFrom>[0]>(null);
  // Every command asks in the same words, so the question alone cannot say a new one came.
  const moved = card?.updated;
  useEffect(() => {
    setReply(null);
    const r = question && online && card ? remote(machine) : null;
    if (!r || !card) return;
    let live = true;
    r.request('ask', { terminal: card.id })
      .then((x) => live && setReply(x))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [question, online, machine, card?.id, moved]);
  return askFrom(reply, question);
}
