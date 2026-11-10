// The blur behind a floating header: what scrolls under it gets softer toward the top of the
// screen, not cut off by one frosted bar. Bands of rising blur stacked from the top (each shorter
// and stronger than the one below) make the steps, and a wash of the page colour over them hides
// where one band ends. No masks: on Android a mask draws its children into a bitmap, which the
// RenderNode blur cannot see through.
import { BlurView } from 'expo-blur';
import type { ComponentRef, RefObject } from 'react';
import { Platform, StyleSheet, View } from 'react-native';
import Svg, { Defs, LinearGradient, Rect, Stop } from 'react-native-svg';
import { themes, useTheme } from './theme';

/** Fraction of the header each band covers from the top, and how strong. */
export const BANDS: readonly { cover: number; intensity: number }[] = [
  { cover: 1, intensity: 55 },
  { cover: 0.75, intensity: 75 },
  { cover: 0.5, intensity: 90 },
  { cover: 0.25, intensity: 100 },
];

export function BlurTop({ target, height }: { target: RefObject<ComponentRef<typeof View> | null>; height: number }) {
  const t = useTheme();
  if (height <= 0) return null;
  return (
    <View pointerEvents="none" style={[StyleSheet.absoluteFill, { height }]}>
      {BANDS.map((b) => (
        <BlurView
          key={b.cover}
          blurTarget={target}
          blurMethod="dimezisBlurViewSdk31Plus"
          intensity={b.intensity}
          tint={t === themes.dark ? 'dark' : 'light'}
          style={[s.band, { height: Math.round(height * b.cover) }]}
        />
      ))}
      <Svg style={StyleSheet.absoluteFill} width="100%" height={height}>
        <Defs>
          <LinearGradient id="wash" x1="0" y1="0" x2="0" y2="1">
            <Stop offset="0" stopColor={t.bg} stopOpacity={Platform.OS === 'android' ? 0.9 : 0.8} />
            <Stop offset="0.5" stopColor={t.bg} stopOpacity={0.6} />
            <Stop offset="1" stopColor={t.bg} stopOpacity={0.35} />
          </LinearGradient>
        </Defs>
        <Rect x="0" y="0" width="100%" height={height} fill="url(#wash)" />
      </Svg>
    </View>
  );
}

const s = StyleSheet.create({
  band: { position: 'absolute', top: 0, left: 0, right: 0 },
});
