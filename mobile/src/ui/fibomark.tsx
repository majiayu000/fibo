// The Fibo mark (docs/site/brand/fibo-icon.svg): an f with a gold dot on the dusk gradient,
// the same as the Mac's Dock and title bar. Heads the home screen.
import { Text, View } from 'react-native';
import Svg, { Circle, Defs, G, LinearGradient, Path, Rect, Stop } from 'react-native-svg';

const F =
  'M63 774Q63 809 88.5 831.0Q114 853 157 853H237L305 1099Q357 1290 447.0 1376.5Q537 1463 677 1463Q767 1463 821.5 1413.5Q876 1364 876 1291Q876 1228 834.0 1190.0Q792 1152 724 1152Q690 1152 666.5 1163.5Q643 1175 625.0 1190.5Q607 1206 589.0 1217.5Q571 1229 549 1229Q529 1229 515.0 1218.0Q501 1207 493 1184Q486 1163 499.5 1140.5Q513 1118 530.5 1088.5Q548 1059 556.0 1015.0Q564 971 548 905L534 852H592Q641 852 668.5 836.0Q696 820 696 777Q696 745 675.5 717.5Q655 690 620.0 673.5Q585 657 544 657H483L326 62Q320 36 316.5 15.0Q313 -6 313 -24Q313 -55 332.0 -75.0Q351 -95 369.5 -116.0Q388 -137 388 -171Q388 -222 344.5 -263.0Q301 -304 233.5 -328.0Q166 -352 94 -352Q9 -352 -13.0 -288.5Q-35 -225 -5 -110L171 561Q183 607 173.0 635.5Q163 664 120 686Q90 706 76.5 726.5Q63 747 63 774Z';

export function FiboMark({ size = 32 }: { size?: number }) {
  return (
    <Svg width={size} height={size} viewBox="0 0 100 100" accessibilityElementsHidden importantForAccessibility="no">
      <Defs>
        <LinearGradient id="dusk" x1="0" y1="0" x2="0.35" y2="1">
          <Stop offset="0" stopColor="#25a9dc" />
          <Stop offset="0.48" stopColor="#5b5fd8" />
          <Stop offset="0.8" stopColor="#e96c8c" />
          <Stop offset="1" stopColor="#f8a95c" />
        </LinearGradient>
      </Defs>
      <Rect width="100" height="100" rx="22.5" fill="url(#dusk)" />
      <G transform="translate(23.53 72.65) scale(0.04077)">
        <G transform="scale(1 -1)">
          <Path d={F} fill="#fff" />
        </G>
        <Circle cx="1141" cy="-1196.5" r="182.2" fill="#f5b82e" />
      </G>
    </Svg>
  );
}

/** The mark and the name, as the head of the home screen. */
export function FiboTitle({ color }: { color: string }) {
  return (
    <View accessible accessibilityRole="header" accessibilityLabel="Fibo" style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
      <FiboMark size={30} />
      <Text style={{ fontSize: 26, lineHeight: 32, fontWeight: '600', letterSpacing: -0.3, color }}>Fibo</Text>
    </View>
  );
}
