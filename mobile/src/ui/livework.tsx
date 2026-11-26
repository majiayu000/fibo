// What a running turn has done since its last message, drawn where that work's folded line
// will stand once it answers: the same 「干了 · 跑了」 line, turning, with its last few calls
// under it, as a CLI draws its calls in place. Before 2026-11-25 this was a box beside the
// messages with 步骤 / 终端 tabs: the calls sat somewhere else while running than once done,
// and the screen's last lines were the CLI's own input box. The screen is the 终端 button.
// Asked every two seconds, only while it runs and this screen is open.
import { memo, useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { remote } from '@/remote/hub';
import { mono } from './kit';
import { type Tools, Worked } from './message';
import { useTheme } from './theme';
import { stepCut, stepWords, took, type ToolCall } from './toolview';
import { tr } from '@/i18n';

const POLL_MS = 2000;
const STEPS = 3;

export type LiveProps = {
  machine: string;
  session: string;
  running: boolean;
  /** Stopped before it answered: asked once, drawn still, with nothing turning. */
  stopped?: boolean;
  /** Where this stretch of work began, Unix seconds: the turn's start or its last message. */
  since: number;
  /** The Agent's last message, Unix seconds: calls before it are folded above it already. */
  after: number;
  /** The card's phrase for it (`运行 cargo build`); none when it has said nothing yet. */
  doing?: string | null;
  workflows?: string[];
  /** The computer answers `toolcalls` with `last`. */
  steps: boolean;
  focused: boolean;
  /** Every call so far, on its own page; undefined when the computer cannot list them. */
  onOpen?: () => void;
};

export const LiveWork = memo(function LiveWork({ machine, session, running, stopped, since, after, doing, workflows, steps, focused, onOpen }: LiveProps) {
  const t = useTheme();
  const [calls, setCalls] = useState<ToolCall[]>([]);
  const [sum, setSum] = useState<Tools>({});
  const [now, setNow] = useState(() => Date.now() / 1000);
  const ticking = running && focused;

  useEffect(() => {
    if (!ticking) return;
    const tick = setInterval(() => setNow(Date.now() / 1000), 1000);
    return () => clearInterval(tick);
  }, [ticking]);

  useEffect(() => {
    // A new message starts a new stretch: what was listed is folded above it now.
    setCalls([]);
    setSum({});
    const once = stopped && focused;
    if (!(ticking || once) || !steps) return;
    let live = true;
    let asking = false;
    const ask = async () => {
      const r = remote(machine);
      if (!r || asking) return;
      asking = true;
      try {
        const page = await r.request('toolcalls', { session, last: STEPS });
        if (!live) return;
        setCalls(page.calls ?? []);
        setSum(page.tools ?? {});
      } catch {
        // The next tick tries again; the line still says what it does.
      } finally {
        asking = false;
      }
    };
    ask();
    const timer = once ? undefined : setInterval(ask, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [machine, session, ticking, stopped, focused, steps, after]);

  const shown = calls.filter((c) => !c.at || c.at >= after);
  const head = doing ? tr('正在{doing}', { doing }) : tr('看它到现在做了什么');

  return (
    <View style={{ gap: 4 }}>
      {running && (
        <Worked
          tools={{ ...sum, took: since > 0 ? Math.max(0, now - since) : 0 }}
          live={head}
          label={tr('{head}，看它到现在做了什么', { head })}
          onPress={onOpen}
        />
      )}
      {stopped && !running && <Worked tools={sum} onPress={onOpen} />}
      {workflows?.map((line, i) => (
        <Text key={i} numberOfLines={1} style={{ fontSize: 12.5, color: t.text }}>◇ {line}</Text>
      ))}
      {(running || stopped) &&
        shown.map((c) => (
          <View key={c.id} style={s.step}>
            <Text style={{ width: 14, textAlign: 'center', fontSize: 12, color: c.failed ? t.err : c.open ? t.run : t.ok }}>
              {c.failed ? '✗' : c.open ? '·' : '✓'}
            </Text>
            <Text numberOfLines={1} ellipsizeMode={stepCut(c)} style={{ flex: 1, fontFamily: mono, fontSize: 12.5, color: c.failed ? t.err : t.text }}>
              {stepWords(c)}
            </Text>
            <Text style={{ fontSize: 12, color: t.dim }}>{c.open ? (running ? tr('还没结束') : tr('停在这里')) : took(c.took_ms)}</Text>
          </View>
        ))}
    </View>
  );
});

const s = StyleSheet.create({
  step: { flexDirection: 'row', alignItems: 'center', gap: 8, minHeight: 22 },
});
