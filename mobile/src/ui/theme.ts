// Colors for light and dark, from the design's new look (docs/phone-app/design.html,
// 「新外观 · 参照 T3 Code」): near-white and #0a0a0a, flat rows, neutral grey own bubbles.
// The system picks light or dark unless settings say otherwise.
import { useColorScheme } from 'react-native';
import { useSettings } from '../store/settings';

const light = {
  /** Page. */
  bg: '#fcfcfc',
  /** Cards, the composer, round buttons. */
  card: '#ffffff',
  /** Grey wells: a command, inline code. */
  well: '#f4f4f5',
  text: '#27272a',
  dim: '#6f6f79',
  /** Card borders. */
  line: '#e4e4e7',
  /** Row separators. */
  sep: 'rgba(228,228,231,0.7)',
  /** The primary button: ink on paper. */
  act: '#27272a',
  onAct: '#fcfcfc',
  bubble: '#f1f1f3',
  link: '#1b4ed8',
  focus: '#1b4ed8',
  // Session states, the desktop's hues (status::of): waiting orange, 在跑 yellow, made a
  // darker amber here so it reads on white, review green, error red.
  wait: '#bb4d00',
  waitBg: '#fcf4e8',
  run: '#a16207',
  ok: '#15803d',
  okBg: '#e8f5ec',
  err: '#c10007',
  errBg: '#fcebec',
  /** The allow button in an approval. */
  allow: '#22c55e',
  onAllow: '#04210f',
  /** Quota bars: under half, past half, past 80%. */
  low: '#22c55e',
  mid: '#e8c547',
  high: '#f0883e',
  empty: '#e4e4e7',
  shadow: 'rgba(0,0,0,0.06)',
  dark: false,
};

const dark: typeof light = {
  bg: '#0a0a0a',
  card: '#111111',
  well: '#1a1b1b',
  text: '#f5f5f5',
  dim: '#8a8a8a',
  line: '#1f1f1f',
  sep: '#1c1c1c',
  act: '#f5f5f5',
  onAct: '#0a0a0a',
  bubble: '#1c1c1e',
  link: '#7aa2ff',
  focus: '#346bf1',
  // The mockup's dark waiting colour was the yellow #ffb900, beside a blue 在跑; with 在跑
  // yellow as on the desktop, waiting takes the desktop's orange so the two stay apart.
  wait: '#f0883e',
  waitBg: '#2a1a0c',
  run: '#e8c547',
  ok: '#4ade80',
  okBg: '#0f2a19',
  err: '#ff6467',
  errBg: '#301214',
  allow: '#22c55e',
  onAllow: '#04210f',
  low: '#3fb950',
  mid: '#e8c547',
  high: '#f0883e',
  empty: '#232328',
  shadow: 'rgba(0,0,0,0.4)',
  dark: true,
};

export type Theme = typeof light;

export function useTheme(): Theme {
  const system = useColorScheme();
  const { appearance } = useSettings();
  const scheme = appearance === 'system' ? system : appearance;
  return scheme === 'dark' ? dark : light;
}

/** "刚刚", "5 分钟前", "3 小时前", or the date. */
export function ago(at: number): string {
  const s = Date.now() / 1000 - at;
  if (s < 60) return '刚刚';
  if (s < 3600) return `${Math.floor(s / 60)} 分钟前`;
  if (s < 86400) return `${Math.floor(s / 3600)} 小时前`;
  const d = new Date(at * 1000);
  return `${d.getMonth() + 1}月${d.getDate()}日`;
}

/** "16:31" today, else "11月3日 16:31". */
export function clock(at: number): string {
  const d = new Date(at * 1000);
  const hm = `${d.getHours()}:${String(d.getMinutes()).padStart(2, '0')}`;
  const today = new Date();
  return d.toDateString() === today.toDateString() ? hm : `${d.getMonth() + 1}月${d.getDate()}日 ${hm}`;
}
