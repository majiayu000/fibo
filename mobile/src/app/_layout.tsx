import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { AppState, Pressable, Text, View } from 'react-native';
import { verify } from '@/actions';
import { start } from '@/remote/hub';
import { Pump } from '@/remote/pump';
import { settings, useSettings } from '@/store/settings';
import { Icon } from '@/ui/icons';
import { useTheme } from '@/ui/theme';

/** With the lock on, the app opens only after Face ID / fingerprint, and again on return. */
function useLocked() {
  const { lock } = useSettings();
  const [locked, setLocked] = useState(true);
  useEffect(() => {
    let live = true;
    settings.ready.then(async () => {
      if (!settings.get().lock) return live && setLocked(false);
      const ok = await verify('打开 Mooring');
      if (live) setLocked(!ok);
    });
    const sub = AppState.addEventListener('change', (s) => {
      if (s === 'background' && settings.get().lock) setLocked(true);
      if (s === 'active' && settings.get().lock) verify('打开 Mooring').then((ok) => ok && setLocked(false));
    });
    return () => {
      live = false;
      sub.remove();
    };
  }, []);
  return [locked && lock, () => verify('打开 Mooring').then((ok) => ok && setLocked(false))] as const;
}

export default function Layout() {
  const t = useTheme();
  const [locked, unlock] = useLocked();
  useEffect(() => {
    // Screenshot builds only; inlined to false otherwise, so the fixture is dropped.
    if (process.env.EXPO_PUBLIC_FIXTURE === '1') require('@/fixture').install();
    start();
  }, []);
  return (
    <>
      <StatusBar style={t.dark ? 'light' : 'dark'} />
      <Pump />
      <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: t.bg } }}>
        <Stack.Screen name="index" />
        <Stack.Screen name="board" options={{ animation: 'none' }} />
        <Stack.Screen name="me" options={{ animation: 'none' }} />
        <Stack.Screen name="search" options={{ animation: 'fade' }} />
        <Stack.Screen name="pair" />
        <Stack.Screen name="chat" />
        <Stack.Screen name="approve" />
        <Stack.Screen name="tools" />
        <Stack.Screen name="tool" />
        <Stack.Screen name="new" options={{ animation: 'slide_from_bottom' }} />
      </Stack>
      {locked && (
        <View style={{ position: 'absolute', inset: 0, backgroundColor: t.bg, alignItems: 'center', justifyContent: 'center', gap: 16 }}>
          <Icon name="face" size={44} color={t.text} />
          <Pressable onPress={unlock} style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 14, backgroundColor: t.act }}>
            <Text style={{ color: t.onAct, fontSize: 16, fontWeight: '600' }}>解锁</Text>
          </Pressable>
        </View>
      )}
    </>
  );
}
