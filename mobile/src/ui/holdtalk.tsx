// Hold to talk, as Doubao does it (user, 2026-11-28: 「学习一下豆包的那个输入的设计」, 「松手，直接发送」):
// hold the empty box and a glow rises from the bottom of the screen, with what is heard so far and a
// wave that follows the voice; slide up and it turns red to drop it; let go to hand the words over.
// Unlike Doubao the words show while talking: an Agent acts on them, so a misheard one is better
// seen before letting go (and the user asked for words as they talk: 「绿的时候，说话是直接出字的」).
import { useEffect, useRef, useState } from 'react';
import { Animated, Easing, type GestureResponderEvent, type StyleProp, StyleSheet, Text, View, type ViewStyle } from 'react-native';
import Svg, { Defs, RadialGradient, Rect, Stop } from 'react-native-svg';
import type { Dictation } from './dictation';
import { tr } from '../i18n';

/** A press shorter than this is a tap: it types instead. */
const HOLD_MS = 220;
/** The finger this far above where it went down drops what was said. */
const AWAY_PX = 80;
/** The glow and the wave read the loudness this often. */
const TICK_MS = 90;
const BARS = 40;
export const GLOW_HEIGHT = 380;

export type Hold = {
  /** The glow is up: held, or the last words on their way back. */
  shown: boolean;
  away: boolean;
  /** Spread on the view that is held. */
  handlers: {
    onStartShouldSetResponder: () => boolean;
    onResponderTerminationRequest: () => boolean;
    onResponderGrant: (e: GestureResponderEvent) => void;
    onResponderMove: (e: GestureResponderEvent) => void;
    onResponderRelease: () => void;
    onResponderTerminate: () => void;
  };
};

export function useHold(d: Dictation, on: { said: (text: string) => void; tap?: () => void; nothing?: () => void }): Hold {
  const [holding, setHolding] = useState(false);
  const [away, setAway] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const y0 = useRef(0);
  const held = useRef(false);
  const awayNow = useRef(false);
  const latest = useRef(on);
  latest.current = on;
  useEffect(() => () => clearTimeout(timer.current), []);

  function end(dropped: boolean) {
    clearTimeout(timer.current);
    if (!held.current) {
      if (!dropped) latest.current.tap?.();
      return;
    }
    held.current = false;
    setHolding(false);
    setAway(false);
    if (dropped || awayNow.current) return d.cancel();
    void d.finish().then((text) => (text.trim() ? latest.current.said(text.trim()) : latest.current.nothing?.()));
  }

  return {
    shown: holding || d.reading,
    away,
    handlers: {
      onStartShouldSetResponder: () => true,
      // A list scrolling under the finger must not take it away mid-sentence.
      onResponderTerminationRequest: () => false,
      onResponderGrant: (e) => {
        y0.current = e.nativeEvent.pageY;
        held.current = false;
        awayNow.current = false;
        timer.current = setTimeout(() => {
          held.current = true;
          setHolding(true);
          d.begin();
        }, HOLD_MS);
      },
      onResponderMove: (e) => {
        if (!held.current) return;
        const a = e.nativeEvent.pageY < y0.current - AWAY_PX;
        if (a === awayNow.current) return;
        awayNow.current = a;
        setAway(a);
      },
      onResponderRelease: () => end(false),
      onResponderTerminate: () => end(true),
    },
  };
}

const BLUE = [
  { at: 0, color: '#2563EB', opacity: 0.95 },
  { at: 0.32, color: '#3B82F6', opacity: 0.8 },
  { at: 0.62, color: '#93C5FD', opacity: 0.45 },
  { at: 1, color: '#BFDBFE', opacity: 0 },
];
const RED = [
  { at: 0, color: '#EF4444', opacity: 0.6 },
  { at: 0.4, color: '#FCA5A5', opacity: 0.45 },
  { at: 0.7, color: '#FEE2E2', opacity: 0.25 },
  { at: 1, color: '#FEE2E2', opacity: 0 },
];

/** The glow, laid over the bottom of the screen (`style` places it there); `letGo` says what letting go does. */
export function VoiceGlow({ hold, dictation, letGo, style }: { hold: Hold; dictation: Dictation; letGo: string; style?: StyleProp<ViewStyle> }) {
  const rise = useRef(new Animated.Value(0)).current;
  const swell = useRef(new Animated.Value(0)).current;
  const read = useRef(dictation.level);
  read.current = dictation.level;
  useEffect(() => {
    Animated.timing(rise, { toValue: 1, duration: 180, easing: Easing.out(Easing.quad), useNativeDriver: true }).start();
    const tick = setInterval(() => {
      Animated.timing(swell, { toValue: read.current(), duration: TICK_MS, easing: Easing.out(Easing.quad), useNativeDriver: true }).start();
    }, TICK_MS);
    return () => clearInterval(tick);
  }, [rise, swell]);
  const { away } = hold;
  const reading = dictation.reading;
  const stops = away ? RED : BLUE;
  const ink = away ? '#4B5563' : '#FFFFFF';
  return (
    <Animated.View
      pointerEvents="none"
      style={[
        s.glow,
        style,
        { opacity: rise, transform: [{ translateY: rise.interpolate({ inputRange: [0, 1], outputRange: [40, 0] }) }] },
      ]}
    >
      <Animated.View style={[StyleSheet.absoluteFill, { transformOrigin: 'bottom', transform: [{ scaleY: swell.interpolate({ inputRange: [0, 1], outputRange: [1, 1.12] }) }] }]}>
        <Svg width="100%" height="100%">
          <Defs>
            <RadialGradient id="voiceglow" cx="50%" cy="100%" rx="70%" ry="100%" fx="50%" fy="100%">
              {stops.map((p) => (
                <Stop key={p.at} offset={p.at} stopColor={p.color} stopOpacity={p.opacity} />
              ))}
            </RadialGradient>
          </Defs>
          <Rect x="0" y="0" width="100%" height="100%" fill="url(#voiceglow)" />
        </Svg>
      </Animated.View>
      <View style={s.inside}>
        <Text numberOfLines={3} ellipsizeMode="head" style={[s.words, { color: ink }]}>
          {dictation.heard || (reading ? '' : tr('在听…'))}
        </Text>
        <Text style={[s.hint, { color: away ? '#DC2626' : 'rgba(255,255,255,0.92)' }]}>
          {reading ? tr('正在转成文字…') : away ? tr('松手取消') : letGo}
        </Text>
        <Wave level={dictation.level} color={away ? '#EF4444' : '#FFFFFF'} still={reading} />
      </View>
    </Animated.View>
  );
}

/** The loudness of the last few seconds, newest on the right. */
function Wave({ level, color, still }: { level: () => number; color: string; still: boolean }) {
  const [bars, setBars] = useState<number[]>(() => new Array(BARS).fill(0));
  const read = useRef(level);
  read.current = level;
  useEffect(() => {
    if (still) return;
    const tick = setInterval(() => setBars((b) => [...b.slice(1), read.current()]), TICK_MS);
    return () => clearInterval(tick);
  }, [still]);
  return (
    <View style={s.wave}>
      {bars.map((v, i) => (
        <View key={i} style={{ width: 2, borderRadius: 1, height: 3 + Math.round(v * 24), backgroundColor: color }} />
      ))}
    </View>
  );
}

const s = StyleSheet.create({
  glow: { position: 'absolute', left: 0, right: 0, bottom: 0, height: GLOW_HEIGHT },
  inside: { flex: 1, justifyContent: 'flex-end', alignItems: 'center', paddingHorizontal: 28, paddingBottom: 30, gap: 16 },
  words: { fontSize: 18, lineHeight: 27, fontWeight: '500', textAlign: 'center' },
  hint: { fontSize: 13, fontWeight: '500' },
  wave: { height: 30, flexDirection: 'row', alignItems: 'center', gap: 3 },
});
