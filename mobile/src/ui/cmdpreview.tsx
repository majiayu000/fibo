// A command cut to a few lines in a row, measuring how many lines all of it takes: a hidden
// copy without the limit reports its lines, so the row knows whether it shows everything.
import { type StyleProp, StyleSheet, Text, type TextStyle, View } from 'react-native';
import { PREVIEW_LINES } from './answers';

export function CmdPreview({ text, style, onLines }: { text: string; style: StyleProp<TextStyle>; onLines: (n: number) => void }) {
  return (
    <View>
      <Text numberOfLines={PREVIEW_LINES} style={style}>
        {text}
      </Text>
      <Text
        style={[style, s.measure]}
        onTextLayout={(e) => onLines(e.nativeEvent.lines.length)}
        accessibilityElementsHidden
        importantForAccessibility="no-hide-descendants"
        pointerEvents="none"
      >
        {text}
      </Text>
    </View>
  );
}

const s = StyleSheet.create({
  measure: { position: 'absolute', top: 0, left: 0, right: 0, opacity: 0 },
});
