// The pieces every screen is built of, sized as the design's new look draws them:
// round 44 pt header buttons, a pill of icon buttons, 36 pt rounded buttons, flat rows
// split by hairlines, group labels with a rule, switches.
import { router } from 'expo-router';
import type { ReactNode } from 'react';
import {
  Platform,
  Pressable,
  type StyleProp,
  StyleSheet,
  Text,
  type TextStyle,
  View,
  type ViewStyle,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Icon, type IconName } from './icons';
import { type Theme, useTheme } from './theme';

/** Chinese titles are regular weight: PingFang's medium reads heavier than the design wants. */
export const TITLE_WEIGHT = '400' as const;

export { mono } from './mono';

/** Web draws a focus ring round inputs; the design's fields show focus by their border. */
export const noOutline = (Platform.OS === 'web' ? { outlineStyle: 'none' } : {}) as TextStyle;

/** The top inset; on web, a phone's status bar, so a browser shows the phone's layout. */
export function useTop() {
  const insets = useSafeAreaInsets();
  return Math.max(insets.top, Platform.OS === 'web' ? 47 : 0);
}

export function useBottom() {
  const insets = useSafeAreaInsets();
  return Math.max(insets.bottom, Platform.OS === 'web' ? 20 : 0);
}

export function Round({
  icon,
  onPress,
  label,
  size = 44,
  filled,
  disabled,
}: {
  icon: IconName;
  onPress: () => void;
  label: string;
  size?: number;
  filled?: boolean;
  disabled?: boolean;
}) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityLabel={label}
      accessibilityRole="button"
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        s.round,
        { width: size, height: size, borderRadius: size / 2, borderColor: filled ? t.act : t.line, backgroundColor: filled ? t.act : t.card },
        (pressed || disabled) && { opacity: disabled ? 0.4 : 0.7 },
      ]}
    >
      <Icon name={icon} size={size > 40 ? 22 : 18} width={size > 40 ? 2 : 2.2} color={filled ? t.onAct : t.text} />
    </Pressable>
  );
}

/** Icon buttons sharing one rounded outline. */
export function Pill({ items }: { items: { icon: IconName; label: string; onPress: () => void; width?: number }[] }) {
  const t = useTheme();
  return (
    <View style={[s.pill, { borderColor: t.line, backgroundColor: t.card }]}>
      {items.map((it) => (
        <Pressable
          key={it.label}
          accessibilityLabel={it.label}
          accessibilityRole="button"
          onPress={it.onPress}
          style={({ pressed }) => [s.pillItem, { width: it.width ?? 40 }, pressed && { opacity: 0.6 }]}
        >
          <Icon name={it.icon} size={20} color={t.text} />
        </Pressable>
      ))}
    </View>
  );
}

export const back = () => (router.canGoBack() ? router.back() : router.replace('/'));

/** A screen's header: back (or close), a title with a line under it, and what goes right. */
export function Nav({
  title,
  sub,
  right,
  close,
  rule,
  big,
  tab,
}: {
  title?: ReactNode;
  sub?: ReactNode;
  right?: ReactNode;
  close?: boolean;
  /** A hairline under it, as on the conversation. */
  rule?: boolean;
  /** An 18 pt title, as on 改动. */
  big?: boolean;
  /** A bottom tab's screen: nothing to go back to. */
  tab?: boolean;
}) {
  const t = useTheme();
  const top = useTop();
  return (
    <View style={[s.nav, { paddingTop: top + 9 }, rule && { borderBottomWidth: 1, borderColor: t.sep, paddingBottom: 10 }]}>
      {!tab && <Round icon={close ? 'close' : 'back'} label={close ? '关闭' : '返回'} onPress={back} />}
      <View style={{ flex: 1, minWidth: 0, gap: 1 }}>
        {typeof title === 'string' ? (
          <Text numberOfLines={1} style={[big ? s.navBig : s.navTitle, { color: t.text }]}>
            {title}
          </Text>
        ) : (
          title
        )}
        {typeof sub === 'string' ? (
          <Text numberOfLines={1} style={[s.navSub, { color: t.dim }]}>
            {sub}
          </Text>
        ) : (
          sub
        )}
      </View>
      {right}
    </View>
  );
}

/** A rounded button: outlined, or filled with ink when `primary`. */
export function Btn({
  label,
  onPress,
  primary,
  icon,
  style,
  tall,
  disabled,
  tone,
}: {
  label: string;
  onPress: () => void;
  primary?: boolean;
  icon?: IconName;
  style?: StyleProp<ViewStyle>;
  /** 44 pt and stretched, as in the diff's summary. */
  tall?: boolean;
  disabled?: boolean;
  tone?: 'danger';
}) {
  const t = useTheme();
  const fg = primary ? t.onAct : tone === 'danger' ? t.err : t.text;
  return (
    <Pressable
      accessibilityRole="button"
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        s.btn,
        { borderColor: primary ? t.act : t.line, backgroundColor: primary ? t.act : t.card },
        tall && { height: 44, flex: 1, justifyContent: 'center' },
        style,
        (pressed || disabled) && { opacity: disabled ? 0.4 : 0.7 },
      ]}
    >
      {icon && <Icon name={icon} size={16} width={2} color={fg} />}
      <Text style={[s.btnText, { color: fg }]}>{label}</Text>
    </Pressable>
  );
}

/** A wide button at the foot of a screen. */
export function Wide({
  label,
  onPress,
  kind = 'primary',
  disabled,
  icon,
  height = 52,
  wrap,
  textStyle,
}: {
  label: string;
  onPress: () => void;
  kind?: 'primary' | 'grey' | 'danger' | 'allow' | 'outline';
  disabled?: boolean;
  icon?: IconName;
  height?: number;
  /** All of the label, however many lines: words the person agrees to must not be cut off. */
  wrap?: boolean;
  textStyle?: StyleProp<TextStyle>;
}) {
  const t = useTheme();
  const look = {
    primary: { bg: t.act, fg: t.onAct, border: t.act },
    grey: { bg: t.well, fg: t.text, border: t.well },
    danger: { bg: t.errBg, fg: t.err, border: t.errBg },
    allow: { bg: t.allow, fg: t.onAllow, border: t.allow },
    outline: { bg: t.card, fg: t.text, border: t.line },
  }[kind];
  return (
    <Pressable
      accessibilityRole="button"
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        s.wide,
        wrap ? { minHeight: height, paddingVertical: 10 } : { height },
        { backgroundColor: look.bg, borderColor: look.border },
        (pressed || disabled) && { opacity: disabled ? 0.4 : 0.75 },
      ]}
    >
      {icon && <Icon name={icon} size={18} width={2} color={look.fg} />}
      <Text style={[s.wideText, { color: look.fg }, textStyle, wrap && { flexShrink: 1, textAlign: 'center' }]} numberOfLines={wrap ? undefined : 1}>
        {label}
      </Text>
    </Pressable>
  );
}

/** 「未发送」, 「稍后提醒 · 2」: a quiet label and a rule; a chevron when it folds. */
export function Group({ label, open, onPress, color }: { label: string; open?: boolean; onPress?: () => void; color?: string }) {
  const t = useTheme();
  const body = (
    <>
      <Text style={[s.groupText, { color: color ?? t.dim }]}>{label}</Text>
      <View style={[s.groupRule, { backgroundColor: t.sep }]} />
      {onPress && (
        <View style={{ transform: [{ rotate: open ? '180deg' : '0deg' }] }}>
          <Icon name="down" size={14} width={2} color={t.dim} />
        </View>
      )}
    </>
  );
  return onPress ? (
    <Pressable onPress={onPress} style={s.group} accessibilityRole="button" accessibilityState={{ expanded: !!open }}>
      {body}
    </Pressable>
  ) : (
    <View style={s.group}>{body}</View>
  );
}

/** A small caps-like label above a block: 「电脑」「最近用过」. */
export function Label({ children, style }: { children: ReactNode; style?: StyleProp<TextStyle> }) {
  const t = useTheme();
  return <Text style={[s.label, { color: t.dim }, style]}>{children}</Text>;
}

/** A bordered rounded block; rows inside split by hairlines. */
export function Sheet({ children, style }: { children: ReactNode; style?: StyleProp<ViewStyle> }) {
  const t = useTheme();
  return <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }, style]}>{children}</View>;
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="switch"
      accessibilityLabel={label}
      accessibilityState={{ checked: on }}
      onPress={() => onChange(!on)}
      style={[s.sw, { backgroundColor: on ? '#34c759' : t.dark ? '#3a3a42' : '#e4e4e7' }]}
    >
      <View style={[s.knob, { left: on ? 22 : 2 }]} />
    </Pressable>
  );
}

/** One line in place of a screen's data: why it is not there. */
export function Notice({ text, tone, style }: { text: string; tone?: 'danger'; style?: StyleProp<TextStyle> }) {
  const t = useTheme();
  return <Text style={[s.notice, { color: tone === 'danger' ? t.err : t.dim }, style]}>{text}</Text>;
}

export const dot = (color: string, size = 8): ViewStyle => ({ width: size, height: size, borderRadius: size / 2, backgroundColor: color });

export function themed(t: Theme) {
  return { page: { flex: 1, backgroundColor: t.bg } as ViewStyle };
}

const s = StyleSheet.create({
  round: { borderWidth: 1, alignItems: 'center', justifyContent: 'center' },
  pill: { flexDirection: 'row', alignItems: 'center', borderWidth: 1, borderRadius: 22, height: 44, paddingHorizontal: 4 },
  pillItem: { height: 40, alignItems: 'center', justifyContent: 'center' },
  nav: { flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 18, paddingBottom: 8 },
  navTitle: { fontSize: 15, fontWeight: TITLE_WEIGHT },
  navBig: { fontSize: 18, fontWeight: TITLE_WEIGHT },
  navSub: { fontSize: 12 },
  btn: {
    height: 36,
    borderRadius: 18,
    borderWidth: 1,
    paddingHorizontal: 14,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  btnText: { fontSize: 14, fontWeight: '500' },
  wide: {
    borderRadius: 14,
    borderWidth: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    paddingHorizontal: 16,
  },
  wideText: { fontSize: 16, fontWeight: '600' },
  group: { flexDirection: 'row', alignItems: 'center', gap: 10, paddingTop: 14, paddingBottom: 6, paddingHorizontal: 18 },
  groupText: { fontSize: 13 },
  groupRule: { flex: 1, height: 1 },
  label: { fontSize: 12, fontWeight: '600', letterSpacing: 0.4 },
  sheet: { borderWidth: 1, borderRadius: 20, overflow: 'hidden' },
  sw: { width: 51, height: 31, borderRadius: 16 },
  knob: { position: 'absolute', top: 2, width: 27, height: 27, borderRadius: 14, backgroundColor: '#fff' },
  notice: { fontSize: 13, textAlign: 'center', paddingVertical: 10, paddingHorizontal: 18, lineHeight: 19 },
});
