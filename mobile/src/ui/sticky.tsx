// A section label that sticks a set distance below the top of its ScrollView, not at the edge:
// the list runs under a floating, blurred header, and the label stops at the header's foot. The
// next label pushes it off. Ported from React Native's ScrollViewStickyHeader (top-sticking only),
// with that distance read from `StickyOffset`. Pass it as the ScrollView's StickyHeaderComponent.
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode, type Ref } from 'react';
import { Animated, type LayoutChangeEvent, Platform } from 'react-native';
import { stickRange } from './stickRange';

/** How far below the ScrollView's top edge a label stops. */
export const StickyOffset = createContext(0);

type Props = {
  children?: ReactNode;
  nextHeaderLayoutY?: number | null;
  onLayout: (e: LayoutChangeEvent) => void;
  scrollAnimatedValue: Animated.Value;
  ref?: Ref<unknown>;
};

type Header = { setNextHeaderY?: (y: number) => void };

export function StickyBelow({ ref, children, nextHeaderLayoutY, onLayout, scrollAnimatedValue }: Props) {
  const offset = useContext(StickyOffset);
  const [box, setBox] = useState<{ y: number; h: number } | null>(null);
  const [next, setNext] = useState(nextHeaderLayoutY);
  // Where the label is now, for touches: a native-driven move does not reach the shadow tree,
  // so without this a tap on a stuck label lands where it used to be (React Native does the same).
  const [settled, setSettled] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const attach = useCallback(
    (r: Header | null) => {
      if (r) r.setNextHeaderY = setNext;
      if (typeof ref === 'function') ref(r);
      else if (ref) (ref as { current: unknown }).current = r;
    },
    [ref],
  );

  const [moved, setMoved] = useState<Animated.AnimatedInterpolation<number>>(() =>
    scrollAnimatedValue.interpolate({ inputRange: [-1, 0], outputRange: [0, 0] }),
  );

  useEffect(() => {
    const range = box ? stickRange(box.y, box.h, next, offset) : { inputRange: [-1, 0], outputRange: [0, 0] };
    const value = scrollAnimatedValue.interpolate(range);
    const wait = Platform.OS === 'android' ? 15 : 64;
    const id = value.addListener(({ value: v }) => {
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setSettled(v), wait);
    });
    setMoved(value);
    return () => {
      value.removeListener(id);
      if (timer.current) clearTimeout(timer.current);
    };
  }, [box, next, offset, scrollAnimatedValue]);

  const passthrough = settled == null ? null : { style: { transform: [{ translateY: settled }] } };
  const AnimatedView = Animated.View as unknown as React.ComponentType<Record<string, unknown>>;
  return (
    <AnimatedView
      collapsable={false}
      ref={attach}
      onLayout={(e: LayoutChangeEvent) => {
        const { y, height } = e.nativeEvent.layout;
        setBox((b) => (b && b.y === y && b.h === height ? b : { y, h: height }));
        onLayout(e);
      }}
      style={{ zIndex: 10, transform: [{ translateY: moved }] }}
      passthroughAnimatedPropExplicitValues={passthrough}
    >
      {children}
    </AnimatedView>
  );
}
