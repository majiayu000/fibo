// A row you can swipe: left reveals the action on its right edge, right the one on its
// left; let go past the threshold and it runs. Built on PanResponder so it needs no
// native module and works on web too.
import { type ReactNode, useRef } from 'react';
import { Animated, PanResponder, StyleSheet, Text, View } from 'react-native';

export type SwipeAction = { label: string; color: string; fg: string; run: () => void };

const THRESHOLD = 90;

// A drag ends in a click on web (and sometimes a press on a phone): the row checks this
// before it opens, so a swipe never doubles as a tap.
let draggedAt = 0;
export const justSwiped = () => Date.now() - draggedAt < 500;

export function Swipe({ children, left, right }: { children: ReactNode; left?: SwipeAction; right?: SwipeAction }) {
  const x = useRef(new Animated.Value(0)).current;
  const acts = useRef({ left, right });
  acts.current = { left, right };
  const pan = useRef(
    PanResponder.create({
      // Capture: a horizontal drag is taken from the row's own press, so it never opens.
      onMoveShouldSetPanResponderCapture: (_, g) => Math.abs(g.dx) > 12 && Math.abs(g.dx) > Math.abs(g.dy) * 1.5,
      onPanResponderTerminationRequest: () => false,
      onPanResponderMove: (_, g) => {
        const { left: l, right: r } = acts.current;
        // Only toward a side that has an action.
        if ((g.dx < 0 && !l) || (g.dx > 0 && !r)) return;
        draggedAt = Date.now();
        x.setValue(g.dx);
      },
      onPanResponderRelease: (_, g) => {
        draggedAt = Date.now();
        const { left: l, right: r } = acts.current;
        const act = g.dx < -THRESHOLD ? l : g.dx > THRESHOLD ? r : undefined;
        if (act) {
          Animated.timing(x, { toValue: g.dx < 0 ? -500 : 500, duration: 160, useNativeDriver: false }).start(() => {
            act.run();
            x.setValue(0);
          });
        } else Animated.spring(x, { toValue: 0, useNativeDriver: false, bounciness: 0 }).start();
      },
      onPanResponderTerminate: () => Animated.spring(x, { toValue: 0, useNativeDriver: false }).start(),
    }),
  ).current;
  return (
    <View style={{ overflow: 'hidden' }}>
      {right && (
        <View style={[StyleSheet.absoluteFill, s.under, { backgroundColor: right.color, justifyContent: 'flex-start' }]}>
          <Text style={[s.label, { color: right.fg }]}>{right.label}</Text>
        </View>
      )}
      {left && (
        <Animated.View
          style={[
            StyleSheet.absoluteFill,
            s.under,
            { backgroundColor: left.color, justifyContent: 'flex-end', opacity: x.interpolate({ inputRange: [-1, 0], outputRange: [1, 0], extrapolate: 'clamp' }) },
          ]}
        >
          <Text style={[s.label, { color: left.fg }]}>{left.label}</Text>
        </Animated.View>
      )}
      <Animated.View {...pan.panHandlers} style={{ transform: [{ translateX: x }] }}>
        {children}
      </Animated.View>
    </View>
  );
}

const s = StyleSheet.create({
  under: { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 22 },
  label: { fontSize: 15, fontWeight: '600' },
});
