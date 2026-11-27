import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, Text, View } from 'react-native';
import { setLanguage, tr, useLang } from '@/i18n';
import { keepCardsIn, start } from '@/remote/hub';
import { Pump } from '@/remote/pump';
import { cardShelf } from '@/store/cardsCache';
import { outboxReady, reloadOutbox } from '@/store/outbox';
import { readSettings, settings } from '@/store/settings';
import { useTheme } from '@/ui/theme';

// A computer away at a cold start still shows the rows it last listed, marked 断开前.
keepCardsIn(cardShelf);

/** The app opens once the saved settings and outbox are read and the computers loaded. Nothing
 *  asks to unlock it: the phone's own screen lock does that (docs/decisions/2026-11-28-phone-no-lock.md). */
function useOpened() {
  const [opened, setOpened] = useState(false);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let live = true;
    async function initialize() {
      try {
        if (attempt) await Promise.all([settings.reload(), reloadOutbox()]);
        await Promise.all([readSettings(), attempt ? Promise.resolve() : outboxReady]);
        if (!live) return;
      } catch {
        if (live) setError(tr('无法读取本机的设置或待发送消息。为保护对话，暂不打开；请重试。'));
        return;
      }
      try {
        await start();
        if (live) setOpened(true);
      } catch {
        if (live) setError(tr('无法读取本机保存的电脑，请重试。'));
      }
    }
    void initialize();
    return () => {
      live = false;
    };
  }, [attempt]);
  const retry = () => {
    setError('');
    setAttempt((n) => n + 1);
  };
  return { opened, error, retry };
}

export default function Layout() {
  const t = useTheme();
  const { opened, error, retry } = useOpened();
  // The language setting reaches the words through i18n's own store; the tree is keyed by it, so
  // every screen draws again in the new language (the stack starts over from the home page).
  const lang = useLang();
  useEffect(() => {
    const sync = () => setLanguage(settings.get().language);
    // useOpened reports read failures and keeps the app closed; this language subscriber
    // must not leave a second rejected promise unhandled during the same failure.
    void settings.ready.then(sync).catch(() => undefined);
    return settings.subscribe(sync);
  }, []);
  useEffect(() => {
    // Screenshot builds only; inlined to false otherwise, so the fixture is dropped.
    // It installs once, never on each foreground; the computers load once the app opens (useOpened).
    if (process.env.EXPO_PUBLIC_FIXTURE === '1' || process.env.EXPO_PUBLIC_FIXTURE === 'promo') require('@/fixture').install();
  }, []);
  return (
    <>
      <StatusBar style={t.dark ? 'light' : 'dark'} />
      {/* Nothing is sent and nothing drawn until the saved settings are read. */}
      {opened && <Pump />}
      {opened && (
        <View style={{ flex: 1 }}>
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
      {!opened && (
        <View style={{ flex: 1, backgroundColor: t.bg, alignItems: 'center', justifyContent: 'center', gap: 16 }}>
          {error ? (
            <>
              <Text style={{ color: t.err, paddingHorizontal: 24, textAlign: 'center' }}>{error}</Text>
              <Pressable accessibilityRole="button" onPress={retry} style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 14, backgroundColor: t.act }}>
                <Text style={{ color: t.onAct, fontSize: 16, fontWeight: '600' }}>{tr('重试读取')}</Text>
              </Pressable>
            </>
          ) : (
            <ActivityIndicator accessibilityLabel={tr('正在读取本机设置')} color={t.text} />
          )}
        </View>
      )}
    </>
  );
}
