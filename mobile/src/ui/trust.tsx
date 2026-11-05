// An Agent asking to be trusted with a folder, before anything else. The machine reads the
// prompt and presses the keys; here the person decides. Nothing trusts on its own.
import { useState } from 'react';
import { Text, View } from 'react-native';
import { answerTrust, oops } from '../actions';
import type { Card } from '../remote/hub';
import { Btn, mono } from './kit';
import { trustWords } from './parse';
import { agentName } from './status';
import { useTheme } from './theme';
import { useSettled } from './useHold';

/** `place` is where the card sits in its list: moved, a tap waits as it does for a new card. */
export function TrustAsk({ machine, card, can, place = '' }: { machine: string; card: Card; can: boolean; place?: string }) {
  const t = useTheme();
  const [busy, setBusy] = useState(false);
  const settled = useSettled(`${card.trust?.folder}|${place}`);
  if (!card.trust) return null;
  const words = trustWords(agentName(card.agent));
  const answer = (yes: boolean) => {
    if (busy || !settled.passes()) return;
    setBusy(true);
    answerTrust(machine, card, yes)
      .catch((e) => oops((e as Error).message))
      .finally(() => setBusy(false));
  };
  return (
    <View style={{ gap: 10 }}>
      <Text style={{ color: t.text, fontSize: 14, lineHeight: 20 }}>{words.ask}</Text>
      <Text selectable numberOfLines={3} style={{ fontFamily: mono, fontSize: 13, color: t.text, backgroundColor: t.well, borderRadius: 10, padding: 10, overflow: 'hidden' }}>
        {card.trust.folder}
      </Text>
      <Text style={{ color: t.dim, fontSize: 13, lineHeight: 19 }}>{words.means}</Text>
      {can ? (
        <View style={{ flexDirection: 'row', gap: 8 }} onStartShouldSetResponder={() => !settled.ready}>
          <Btn primary label="信任这个文件夹" disabled={busy || !settled.ready} onPress={() => answer(true)} />
          <Btn label="不信任" disabled={busy || !settled.ready} onPress={() => answer(false)} />
        </View>
      ) : (
        <Text style={{ color: t.dim, fontSize: 13 }}>这台手机的权限不够，到电脑上回答。</Text>
      )}
    </View>
  );
}
