// A message waiting in the Agent's queue, tapped: send it now instead of after the step at
// hand. The Agent's queue goes as one, so every queued message goes with it and the sheet
// says so. Taking one back is not offered: Claude's ↑ takes the whole queue back into its box,
// and putting the rest back depends on how its box edits (measured 2026-11-09).
import { Text, View } from 'react-native';
import { Frame } from './sheets';
import { Wide } from './kit';
import { useTheme } from './theme';
import { tr } from '../i18n';

export function QueuedSheet({ texts, onSendNow, onClose }: { texts: string[]; onSendNow: () => void; onClose: () => void }) {
  const t = useTheme();
  const many = texts.length > 1;
  return (
    <Frame visible onClose={onClose}>
      <View style={{ gap: 12, paddingTop: 4 }}>
        <View style={{ borderLeftWidth: 3, borderColor: t.run, paddingLeft: 10, gap: 4 }}>
          {texts.map((x, i) => (
            <Text key={i} numberOfLines={2} style={{ color: t.dim, fontSize: 14, lineHeight: 19 }}>
              {x}
            </Text>
          ))}
        </View>
        <Text style={{ color: t.text, fontSize: 14, lineHeight: 20 }}>
          {many ? tr('排着的 {n} 句会一起发出去，不等手头这一步做完，它马上就会看到。', { n: texts.length }) : tr('不等手头这一步做完，它马上就会看到。')}
        </Text>
        <Wide label={tr('立刻发送')} onPress={onSendNow} height={48} />
        <Wide label={tr('取消')} kind="grey" onPress={onClose} height={46} />
      </View>
    </Frame>
  );
}
