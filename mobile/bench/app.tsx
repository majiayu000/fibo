// The Markdown benchmark screen. Built only with EXPO_PUBLIC_MD_BENCH=1 (`index.ts`); a normal
// bundle never reaches this file. Opened with a URL that names what to run:
//   moorbench://run?engine=B&scene=render&count=50&share=0.31
// Results go to logcat as `MD_BENCH {...}`.
import { registerRootComponent } from 'expo';
import { memo, useEffect, useRef, useState } from 'react';
import { FlatList, Linking, ScrollView, Text, View } from 'react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { Markdown } from '../src/ui/markdown';
import { parseMarkdown } from '../src/ui/md/parse';
import { themes, useTheme } from '../src/ui/theme';
import { GALLERY } from './gallery';
import { LegacyMarkdown } from './legacy-entry';
import { longest, replies } from './synth';

const log = (o: unknown) => console.log(`MD_BENCH ${JSON.stringify(o)}`);
const now = () => performance.now();

const median = (a: number[]) => [...a].sort((x, y) => x - y)[a.length >> 1];
const fix = (n: number) => Math.round(n * 100) / 100;

function parseScene() {
  const result: Record<string, unknown> = { scene: 'parse', hermes: !!(globalThis as { HermesInternal?: unknown }).HermesInternal };
  const long = longest();
  let t = now();
  parseMarkdown('**首次**');
  result.firstCallMs = fix(now() - t);
  const runs: number[] = [];
  for (let i = 0; i < 15; i++) {
    t = now();
    parseMarkdown(long);
    runs.push(now() - t);
  }
  result.longChars = long.length;
  result.longFirstMs = fix(runs[0]);
  result.longMedianMs = fix(median(runs));
  result.longMaxMs = fix(Math.max(...runs));
  for (const [name, share] of [['first50mix', 0.31], ['first50final', 1]] as const) {
    const each: number[] = [];
    for (let seed = 1; seed <= 7; seed++) {
      const data = replies(50, seed * 101, share);
      t = now();
      for (const d of data) parseMarkdown(d.text);
      each.push(now() - t);
    }
    result[`${name}MedianMs`] = fix(median(each));
    result[`${name}MaxMs`] = fix(Math.max(...each));
    result[`${name}Chars`] = replies(50, 101, share).reduce((n, d) => n + d.text.length, 0);
  }
  const root = parseMarkdown('**结论：**后面') as unknown as { children: { children: { type: string; children?: { value: string }[]; value?: string }[] }[] };
  result.cjkBold = root.children[0].children.map((c) => (c.type === 'strong' ? `<b>${c.children?.[0]?.value}</b>` : c.value)).join('');
  log(result);
}

const Row = memo(function Row({ text, engine, seq }: { text: string; engine: string; seq: number }) {
  const t = useTheme();
  return (
    <View style={{ gap: 8 }}>
      {engine === 'B' ? (
        <Markdown text={text} t={t} color={t.text} />
      ) : (
        <LegacyMarkdown text={text} t={t} color={t.text} />
      )}
      <Text style={{ color: t.dim, fontSize: 12 }}>{`16:${String(seq % 60).padStart(2, '0')} · Claude Code`}</Text>
    </View>
  );
});

function ListScene({ engine, count, share, seed }: { engine: string; count: number; share: number; seed: number }) {
  const t = useTheme();
  const data = useRef(replies(count, seed, share)).current;
  const start = useRef(now());
  const first = useRef<number | null>(null);
  const last = useRef(0);
  const height = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const report = () => {
    requestAnimationFrame(() => {
      log({
        scene: 'render',
        engine,
        count,
        share,
        seed,
        firstContentMs: fix(first.current ?? -1),
        settledMs: fix(last.current),
        contentHeight: Math.round(height.current),
        chars: data.reduce((n, d) => n + d.text.length, 0),
      });
    });
  };
  return (
    <FlatList
      inverted
      style={{ flex: 1, backgroundColor: t.bg }}
      data={data}
      keyExtractor={(d) => String(d.seq)}
      renderItem={({ item }) => <Row text={item.text} engine={engine} seq={item.seq} />}
      contentContainerStyle={{ paddingHorizontal: 18, paddingVertical: 14, gap: 10 }}
      initialNumToRender={8}
      maxToRenderPerBatch={6}
      windowSize={9}
      onContentSizeChange={(_, h) => {
        const at = now() - start.current;
        if (first.current === null && h > 0) first.current = at;
        last.current = at;
        height.current = h;
        if (timer.current) clearTimeout(timer.current);
        timer.current = setTimeout(report, 1500);
      }}
    />
  );
}

/** One reply drawn by the old or new renderer, in either theme, for screenshots. */
function Gallery({ engine, page, scheme }: { engine: string; page: number; scheme: 'light' | 'dark' }) {
  const t = themes[scheme];
  const Engine = engine === 'B' ? Markdown : LegacyMarkdown;
  return (
    <ScrollView style={{ flex: 1, backgroundColor: t.bg }} contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 56, paddingBottom: 30, gap: 8 }}>
      <Engine text={GALLERY[page]} t={t} color={t.text} />
      <Text style={{ color: t.dim, fontSize: 12 }}>16:33 · Claude Code</Text>
    </ScrollView>
  );
}

function App() {
  const [url, setUrl] = useState<URL | null | 'none'>(null);
  useEffect(() => {
    Linking.getInitialURL().then((u) => {
      const parsedUrl = u ? new URL(u.replace('moorbench://', 'http://bench/')) : null;
      setUrl(parsedUrl ?? 'none');
      if (parsedUrl?.searchParams.get('scene') === 'parse') setTimeout(parseScene, 300);
    });
  }, []);
  if (url === null) return null;
  if (url === 'none' || url.searchParams.get('scene') === 'parse') return <View style={{ flex: 1, backgroundColor: '#888' }} />;
  const q = url.searchParams;
  if (q.get('scene') === 'gallery') return <Gallery engine={q.get('engine') ?? 'B'} page={Number(q.get('page') ?? 0)} scheme={q.get('theme') === 'dark' ? 'dark' : 'light'} />;
  return (
    <SafeAreaProvider>
      <ListScene engine={q.get('engine') ?? 'B'} count={Number(q.get('count') ?? 50)} share={Number(q.get('share') ?? 0.31)} seed={Number(q.get('seed') ?? 11)} />
    </SafeAreaProvider>
  );
}

registerRootComponent(App);
