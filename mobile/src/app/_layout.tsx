import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useRef, useState } from 'react';
import { ActivityIndicator, AppState, Pressable, Text, View } from 'react-native';
import { verify } from '@/actions';
import { setLanguage, tr, useLang } from '@/i18n';
import { start } from '@/remote/hub';
import { Pump } from '@/remote/pump';
import { outboxReady, reloadOutbox } from '@/store/outbox';
import { readSettings, settings } from '@/store/settings';
import { Icon } from '@/ui/icons';
import { useTheme } from '@/ui/theme';

/** With the lock on, the app opens only after Face ID / fingerprint, and again on return. */
function useLocked() {
  const [locked, setLocked] = useState(true);
  const [ready, setReady] = useState(false);
  const [opened, setOpened] = useState(false);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const unlock = useRef(() => {});
  useEffect(() => {
    let live = true;
    let generation = 0;
    let authenticating = false;
    let loaded = false;
    const open = async () => {
      if (!loaded || authenticating) return;
      authenticating = true;
      const stamp = generation;
      const ok = await verify(tr('打开 Fibo'));
      try {
        if (ok && live && generation === stamp && AppState.currentState !== 'background') {
          await start();
          if (live && generation === stamp && AppState.currentState !== 'background') {
            setOpened(true);
            setLocked(false);
          }
        }
      } catch {
        if (live) setError(tr('无法读取本机保存的电脑，请重试。'));
      } finally {
        authenticating = false;
      }
    };
    unlock.current = () => {
      void open();
    };
    async function initialize() {
      try {
        if (attempt) await Promise.all([settings.reload(), reloadOutbox()]);
        await Promise.all([readSettings(), attempt ? Promise.resolve() : outboxReady]);
        if (!live) return;
        loaded = true;
        setReady(true);
        await open();
      } catch {
        if (live) setError(tr('无法读取本机的设置或待发送消息。为保护对话，暂不打开；请重试。'));
      }
    }
    void initialize();
    const sub = AppState.addEventListener('change', (s) => {
      if (s === 'background') generation++;
      if (s !== 'active' && settings.get().lock) setLocked(true);
      if (s === 'active') void open();
    });
    return () => {
      live = false;
      sub.remove();
    };
  }, [attempt]);
  const retry = () => {
    setError('');
    setReady(false);
    setAttempt((n) => n + 1);
  };
  return { locked, ready, opened, error, unlock: () => unlock.current(), retry };
}

export default function Layout() {
  const t = useTheme();
  const { locked, ready, opened, error, unlock, retry } = useLocked();
  // The language setting reaches the words through i18n's own store; the tree is keyed by it, so
  // every screen draws again in the new language (the stack starts over from the home page).
  const lang = useLang();
  useEffect(() => {
    const sync = () => setLanguage(settings.get().language);
    settings.ready.then(sync);
    return settings.subscribe(sync);
  }, []);
  useEffect(() => {
    // Screenshot builds only; inlined to false otherwise, so the fixture is dropped.
    // It installs once, never on each foreground; the computers load once the app opens (useLocked).
    if (process.env.EXPO_PUBLIC_FIXTURE === '1' || process.env.EXPO_PUBLIC_FIXTURE === 'promo') require('@/fixture').install();
  }, []);
  return (
    <>
      <StatusBar style={t.dark ? 'light' : 'dark'} />
      {/* Nothing is sent and nothing drawn until the saved settings are read and the lock passed. */}
      {opened && !locked && <Pump />}
      {opened && (
        <View style={{ flex: 1 }} pointerEvents={locked ? 'none' : 'auto'} accessibilityElementsHidden={locked} importantForAccessibility={locked ? 'no-hide-descendants' : 'auto'}>
          <Stack key={lang} screenOptions={{ headerShown: false, contentStyle: { backgroundColor: t.bg } }}>
            <Stack.Screen name="index" />
            <Stack.Screen name="todos" options={{ animation: 'none' }} />
            <Stack.Screen name="todo" />
            <Stack.Screen name="todonew" options={{ animation: 'slide_from_bottom' }} />
            <Stack.Screen name="me" options={{ animation: 'none' }} />
            <Stack.Screen name="search" options={{ animation: 'fade' }} />
            <Stack.Screen name="pair" />
            <Stack.Screen name="chat" />
            <Stack.Screen name="approve" />
            <Stack.Screen name="tools" />
            <Stack.Screen name="tool" />
            <Stack.Screen name="new" options={{ animation: 'slide_from_bottom' }} />
          </Stack>
        </View>
      )}
      {locked && (
        <View style={{ position: 'absolute', inset: 0, backgroundColor: t.bg, alignItems: 'center', justifyContent: 'center', gap: 16 }}>
          <Icon name="face" size={44} color={t.text} />
          {!!error && <Text style={{ color: t.err, paddingHorizontal: 24, textAlign: 'center' }}>{error}</Text>}
          {ready || error ? (
            <Pressable onPress={error ? retry : unlock} style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 14, backgroundColor: t.act }}>
              <Text style={{ color: t.onAct, fontSize: 16, fontWeight: '600' }}>{error ? tr('重试读取') : tr('解锁')}</Text>
            </Pressable>
          ) : (
            <ActivityIndicator accessibilityLabel={tr('正在读取本机设置')} color={t.text} />
          )}
        </View>
      )}
    </>
  );
}
