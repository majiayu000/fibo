// The strip above the composer while an Agent waits on you: what it asks and a way to answer
// it from here (see waits.ts for which way).
import { Text, View } from 'react-native';
import { Btn } from './kit';
import { useTheme } from './theme';
import type { WaitView } from './waits';

export function WaitPanel({
  view,
  canKey,
  onKeys,
  onTerminal,
}: {
  view: WaitView;
  /** The computer takes keys from this phone. */
  canKey: boolean;
  onKeys: (keys: string[]) => void;
  onTerminal: () => void;
}) {
  const t = useTheme();
  return (
    <View style={{ paddingHorizontal: 18, paddingBottom: 10, gap: 8 }}>
      <Text numberOfLines={6} style={{ color: t.wait, fontSize: 13, lineHeight: 19 }}>
        {view.text}
      </Text>
      {!!view.computer && <Text style={{ color: t.dim, fontSize: 13, lineHeight: 19 }}>{view.computer}</Text>}
      {!view.options.length && !view.computer && (
        <Text style={{ color: t.dim, fontSize: 12, lineHeight: 17 }}>先点「看终端」看清楚，再用 ↑ ↓ 移到要的那项、按回车</Text>
      )}
      <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
        {canKey && view.options.map((o) => <Btn key={o.key} label={o.label} onPress={() => onKeys([o.key])} />)}
        <Btn label="看终端" onPress={onTerminal} />
        {canKey && view.keys && (
          <>
            <Btn label="↑" onPress={() => onKeys(['up'])} />
            <Btn label="↓" onPress={() => onKeys(['down'])} />
            <Btn label="回车" onPress={() => onKeys(['enter'])} />
          </>
        )}
        {canKey && <Btn label="Esc" onPress={() => onKeys(['esc'])} />}
      </View>
    </View>
  );
}
