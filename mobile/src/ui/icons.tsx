// The design's line icons (docs/phone-app/design.html), drawn with react-native-svg on a
// 24 unit grid; each is the mockup's own path.
import { Text, View } from 'react-native';
import Svg, { Circle, Defs, LinearGradient, Path, Rect, Stop } from 'react-native-svg';

const paths = {
  back: <Path d="M15 5l-7 7 7 7" />,
  home: <Path d="M4 11l8-7 8 7v9H4z" />,
  person: <Path d="M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21c1.500-4 4.500-6 8-6s6.500 2 8 6" />,
  close: <Path d="M6 6l12 12M18 6L6 18" />,
  plus: <Path d="M12 5v14M5 12h14" />,
  /** The 待办 tab: a ticked box. */
  todo: (
    <>
      <Rect x="4" y="4" width="16" height="16" rx="3" />
      <Path d="M8.5 12.5l2.5 2.5 4.5-5.5" />
    </>
  ),
  computer: (
    <>
      <Rect x="3" y="4" width="18" height="12" rx="2" />
      <Path d="M8 20h8M12 16v4" />
    </>
  ),
  quota: <Path d="M5 20V10M10 20V4M15 20v-7M20 20v-4" />,
  gear: (
    <>
      <Circle cx="12" cy="12" r="3" />
      <Path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1" />
    </>
  ),
  down: <Path d="M6 9l6 6 6-6" />,
  right: <Path d="M9 6l6 6-6 6" />,
  /** The return key: a queued message goes now. */
  enter: <Path d="M20 5v6a4 4 0 0 1-4 4H5M9 11l-4 4 4 4" />,
  folder: <Path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />,
  search: (
    <>
      <Circle cx="11" cy="11" r="7" />
      <Path d="M20 20l-3.5-3.5" />
    </>
  ),
  lock: (
    <>
      <Rect x="5" y="11" width="14" height="9" rx="2" />
      <Path d="M8 11V8a4 4 0 0 1 8 0v3" />
    </>
  ),
  check: <Path d="M5 12l5 5 9-10" />,
  up: <Path d="M12 19V5M5 12l7-7 7 7" />,
  face: <Path d="M4 8V6a2 2 0 0 1 2-2h2M16 4h2a2 2 0 0 1 2 2v2M20 16v2a2 2 0 0 1-2 2h-2M8 20H6a2 2 0 0 1-2-2v-2M9 10v1M15 10v1M10 15a3 3 0 0 0 4 0" />,
  diff: (
    <>
      <Path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
      <Path d="M14 3v6h6M12 11v6M9 14h6" />
    </>
  ),
  terminal: (
    <>
      <Rect x="3" y="4" width="18" height="16" rx="2" />
      <Path d="M7 9l3 3-3 3M12 15h5" />
    </>
  ),
  prompt: <Path d="M4 17l6-6-6-6M12 19h8" />,
  file: (
    <>
      <Path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
      <Path d="M14 3v6h6" />
    </>
  ),
  globe: (
    <>
      <Circle cx="12" cy="12" r="9" />
      <Path d="M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18" />
    </>
  ),
  shield: <Path d="M12 3l7 3v5c0 5-3.5 8-7 10-3.5-2-7-5-7-10V6z" />,
  stop: <Rect x="7" y="7" width="10" height="10" rx="1.5" />,
  camera: (
    <>
      <Path d="M4 8h3l2-3h6l2 3h3v11H4z" />
      <Circle cx="12" cy="13" r="3.5" />
    </>
  ),
  image: (
    <>
      <Rect x="4" y="4" width="16" height="16" rx="3" />
      <Circle cx="9" cy="9.5" r="1.6" />
      <Path d="M20 15l-4.5-4.5L6 20" />
    </>
  ),
  alert: (
    <>
      <Path d="M12 8v5M12 16.5v.5" />
      <Circle cx="12" cy="12" r="9" />
    </>
  ),
};

export type IconName = keyof typeof paths;

export function Icon({
  name,
  size = 20,
  color,
  width = 1.8,
}: {
  name: IconName;
  size?: number;
  color: string;
  width?: number;
}) {
  return (
    <Svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={name === 'stop' ? color : 'none'}
      stroke={color}
      strokeWidth={width}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {paths[name]}
    </Svg>
  );
}

/** Gradient pairs for spaces, as the desktop paints its space dots. */
const GRADIENTS: [string, string][] = [
  ['#7f5af0', '#2cb67d'],
  ['#f25f4c', '#ff8906'],
  ['#3a86ff', '#8338ec'],
  ['#06d6a0', '#118ab2'],
  ['#ef476f', '#ffd166'],
  ['#4cc9f0', '#4361ee'],
];

export function spaceGradient(key: string): [string, string] {
  let h = 0;
  for (const c of key) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return GRADIENTS[h % GRADIENTS.length];
}

/** A rounded square in the space's gradient, its initial inside: the row's project mark. */
export function SpaceMark({ name, id, size = 16, radius = 4 }: { name: string; id: string; size?: number; radius?: number }) {
  const [a, b] = spaceGradient(id);
  const letter = (name.trim()[0] ?? '?').toUpperCase();
  const gid = `g${id.replace(/[^A-Za-z0-9]/g, '')}${size}`;
  return (
    <View style={{ width: size, height: size, alignItems: 'center', justifyContent: 'center' }}>
      <Svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} style={{ position: 'absolute' }}>
        <Defs>
          <LinearGradient id={gid} x1="0" y1="0" x2="1" y2="1">
            <Stop offset="0" stopColor={a} />
            <Stop offset="1" stopColor={b} />
          </LinearGradient>
        </Defs>
        <Rect x="0" y="0" width={size} height={size} rx={radius} fill={`url(#${gid})`} />
      </Svg>
      <Text style={{ color: '#fff', fontSize: Math.round(size * 0.56), fontWeight: '700', lineHeight: size, includeFontPadding: false }}>{letter}</Text>
    </View>
  );
}

/** A round dot in the space's gradient. */
export function SpaceDot({ id, size = 8 }: { id: string; size?: number }) {
  const [a, b] = spaceGradient(id);
  const gid = `d${id.replace(/[^A-Za-z0-9]/g, '')}${size}`;
  return (
    <Svg width={size} height={size}>
      <Defs>
        <LinearGradient id={gid} x1="0" y1="0" x2="1" y2="1">
          <Stop offset="0" stopColor={a} />
          <Stop offset="1" stopColor={b} />
        </LinearGradient>
      </Defs>
      <Circle cx={size / 2} cy={size / 2} r={size / 2} fill={`url(#${gid})`} />
    </Svg>
  );
}
