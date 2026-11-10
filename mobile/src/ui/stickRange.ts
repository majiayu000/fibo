// Where a section label sits as the list scrolls, for StickyBelow (sticky.tsx). Kept apart from
// the component so it can be tested without React Native.

/** Scroll position → how far the label moves down to stay put. Inputs must not decrease. */
export function stickRange(y: number, h: number, next: number | null | undefined, offset: number) {
  const stick = Math.max(0, y - offset);
  const input = [-1, 0, stick];
  const output = [0, 0, 0];
  // The next label reaches this one's foot: from there it scrolls away with the list.
  const collision = (next ?? 0) - h - offset;
  if (collision >= stick) {
    input.push(collision, collision + 1);
    output.push(collision - stick, collision - stick);
  } else {
    input.push(stick + 1);
    output.push(1);
  }
  return { inputRange: input, outputRange: output };
}
