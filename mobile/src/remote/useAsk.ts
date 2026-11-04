// The approval a card shows, read off its terminal: what it wants to run, and whether the
// screen offers "don't ask again". Only the approval needs the screen's text.
import { useEffect, useState } from 'react';
import { type Ask, readAsk } from '../ui/parse';
import { type Card, remote } from './hub';

export function useAsk(machine: string, card: Card | undefined, online: boolean): Ask & { screen: string } {
  const question = card?.approval ?? '';
  const [screen, setScreen] = useState('');
  useEffect(() => {
    const r = question && online && card ? remote(machine) : null;
    if (!r || !card) return setScreen('');
    let live = true;
    r.request('read', { terminal: card.id, lines: 24 })
      .then((x) => live && setScreen(String(x.text ?? '').replace(/\s+$/, '')))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [question, online, machine, card?.id]);
  return { ...readAsk(screen, question), screen };
}
