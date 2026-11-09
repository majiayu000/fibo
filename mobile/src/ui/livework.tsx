// What a running turn is doing, under its messages: its last few calls by default, or the
// computer's screen as it is now. Between its messages a turn can work for minutes, and one
// line saying 「看它到现在做了什么」 read as nothing happening (2026-11-09). Asked every two
// seconds, only while it runs and this screen is open.
import { memo, useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';
import { remote } from '@/remote/hub';
import { Icon } from './icons';
import { mono } from './kit';
import { useTheme } from './theme';
import { lastLines, took, type ToolCall } from './toolview';

const POLL_MS = 2000;
const STEPS = 3;
const LINES = 8;

type Mode = 'steps' | 'screen';
/** The last choice, for the next conversation opened while the app runs. */
let chosen: Mode = 'steps';

/** The steps' words: what the Agent said a call does, else what it ran. */
const words = (c: ToolCall) => c.about || c.label || c.name;

export const LiveWork = memo(function LiveWork({
  machine,
  terminal,
  session,
  doing,
  steps: canSteps,
  screen: canScreen,
  focused,
  onOpen,
}: {
  machine: string;
  terminal: string;
  session: string;
  /** The card's phrase for it (`运行 cargo build`); none when it has said nothing yet. */
  doing?: string | null;
  /** The computer answers `toolcalls` with `last`. */
  steps: boolean;
  /** The computer answers `read`. */
  screen: boolean;
  focused: boolean;
  /** Every call so far, on its own page; undefined when the computer cannot list them. */
  onOpen?: () => void;
}) {
  const t = useTheme();
  const [mode, setMode] = useState<Mode>(canSteps ? chosen : 'screen');
  const [calls, setCalls] = useState<ToolCall[]>([]);
  const [lines, setLines] = useState<string[]>([]);
  const shown: Mode | null = mode === 'steps' && canSteps ? 'steps' : canScreen && mode === 'screen' ? 'screen' : null;

  useEffect(() => {
    if (!focused || !shown) return;
    let live = true;
    let asking = false;
    const ask = async () => {
      const r = remote(machine);
      if (!r || asking) return;
      asking = true;
      try {
        if (shown === 'steps') {
          const page = await r.request('toolcalls', { session, last: STEPS });
          if (live) setCalls(page.calls ?? []);
        } else {
          const read = await r.request('read', { terminal, lines: LINES + 4 });
          if (live) setLines(lastLines(String(read.text ?? ''), LINES));
        }
      } catch {
        // The next tick tries again; the line above still says what it does.
      } finally {
        asking = false;
      }
    };
    ask();
    const timer = setInterval(ask, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [machine, terminal, session, shown, focused]);

  const latest = calls[calls.length - 1];
  const head = doing ? `正在${doing}` : latest ? words(latest) : '看它到现在做了什么';
  const pick = (m: Mode) => {
    chosen = m;
    setMode(m);
  };

  return (
    <View style={[s.box, { borderColor: t.sep }]}>
      <View style={s.head}>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={`${head}，看它到现在做了什么`}
          disabled={!onOpen}
          onPress={onOpen}
          style={s.now}
        >
          <ActivityIndicator size="small" color={t.dim} />
          <Text numberOfLines={1} style={{ flex: 1, fontSize: 13, color: t.text }}>
            {head}
          </Text>
          {!!onOpen && <Icon name="right" size={14} width={2} color={t.dim} />}
        </Pressable>
        {canSteps && canScreen && (
          <View accessibilityRole="tablist" style={[s.seg, { backgroundColor: t.well }]}>
            {(['steps', 'screen'] as const).map((m) => (
              <Pressable
                key={m}
                accessibilityRole="tab"
                accessibilityState={{ selected: mode === m }}
                hitSlop={6}
                onPress={() => pick(m)}
                style={[s.segOne, mode === m && { backgroundColor: t.card }]}
              >
                <Text style={{ fontSize: 12, color: mode === m ? t.text : t.dim }}>{m === 'steps' ? '步骤' : '终端'}</Text>
              </Pressable>
            ))}
          </View>
        )}
      </View>
      {shown === 'steps' &&
        calls.map((c) => (
          <View key={c.id} style={s.step}>
            <Text style={{ width: 14, textAlign: 'center', fontSize: 12, color: c.failed ? t.err : c.open ? t.run : t.ok }}>
              {c.failed ? '✗' : c.open ? '·' : '✓'}
            </Text>
            <Text numberOfLines={1} ellipsizeMode="middle" style={{ flex: 1, fontFamily: mono, fontSize: 12.5, color: c.failed ? t.err : t.text }}>
              {words(c)}
            </Text>
            <Text style={{ fontSize: 12, color: t.dim }}>{c.open ? '还没结束' : took(c.took_ms)}</Text>
          </View>
        ))}
      {shown === 'screen' && lines.length > 0 && (
        <View style={[s.screen, { backgroundColor: t.well }]}>
          {lines.map((l, i) => (
            <Text key={i} numberOfLines={1} ellipsizeMode="clip" style={{ fontFamily: mono, fontSize: 11.5, lineHeight: 16, color: t.text }}>
              {l || ' '}
            </Text>
          ))}
        </View>
      )}
    </View>
  );
});

const s = StyleSheet.create({
  box: { borderWidth: 1, borderRadius: 14, paddingHorizontal: 10, paddingVertical: 8, gap: 4 },
  head: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  now: { flex: 1, minWidth: 0, flexDirection: 'row', alignItems: 'center', gap: 6, minHeight: 28 },
  seg: { flexDirection: 'row', borderRadius: 8, padding: 2 },
  segOne: { paddingHorizontal: 9, paddingVertical: 3, borderRadius: 6 },
  step: { flexDirection: 'row', alignItems: 'center', gap: 8, minHeight: 22 },
  screen: { borderRadius: 8, paddingHorizontal: 8, paddingVertical: 6, marginTop: 2 },
});
