// The bottom tabs: 主页, 待办, 我的. They sit on the screens rather than a tab navigator, so
// routes and links into a conversation stay as they are: home stays mounted underneath,
// the system back from a tab returns to it.
import { router } from 'expo-router';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { Icon, type IconName } from './icons';
import { back, useBottom } from './kit';
import { useTheme } from './theme';

export type Tab = 'home' | 'todos' | 'me';

const TABS: { key: Tab; label: string; icon: IconName; path: '/' | '/todos' | '/me' }[] = [
  { key: 'home', label: '主页', icon: 'home', path: '/' },
  { key: 'todos', label: '待办', icon: 'todo', path: '/todos' },
  { key: 'me', label: '我的', icon: 'person', path: '/me' },
];

function go(from: Tab, to: (typeof TABS)[number]) {
  if (to.key === from) return;
  if (to.key === 'home') back();
  else if (from === 'home') router.push(to.path);
  else router.replace(to.path);
}

export const TAB_HEIGHT = 56;

export function TabBar({ on }: { on: Tab }) {
  const t = useTheme();
  const bottom = useBottom();
  return (
    <View style={[s.bar, { borderColor: t.line, backgroundColor: t.card, paddingBottom: bottom }]}>
      {TABS.map((tab) => {
        const here = tab.key === on;
        const color = here ? t.text : t.dim;
        return (
          <Pressable
            key={tab.key}
            accessibilityRole="tab"
            accessibilityLabel={tab.label}
            accessibilityState={{ selected: here }}
            onPress={() => go(on, tab)}
            style={({ pressed }) => [s.tab, pressed && { opacity: 0.6 }]}
          >
            <Icon name={tab.icon} size={22} width={here ? 2.2 : 1.8} color={color} />
            <Text style={[s.label, { color }, here && { fontWeight: '600' }]}>{tab.label}</Text>
          </Pressable>
        );
      })}
    </View>
  );
}

const s = StyleSheet.create({
  bar: { flexDirection: 'row', borderTopWidth: 1 },
  tab: { flex: 1, height: TAB_HEIGHT, alignItems: 'center', justifyContent: 'center', gap: 2 },
  label: { fontSize: 11 },
});
