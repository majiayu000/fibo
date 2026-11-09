// The home screen: every paired computer's conversations, what needs you first as cards you can
// answer here. What runs and what waits to be checked fold under them, so none of it pushes the
// other off the screen. Swipe left for 已处理, right for 稍后提醒, with a moment to undo. What you
// wrote while a computer was away waits under 「未发送」. Chips under the title show one space
// alone; spaces put aside on the computer are named at the foot, each can be taken back.
import { router } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { approve, deny, oops, settle } from '@/actions';
import { type Card, type Machine, reconnect, supports, useHub } from '@/remote/hub';
import { useAsk } from '@/remote/useAsk';
import { setInboxView, useInboxView } from '@/store/inboxView';
import { drop, useOutbox } from '@/store/outbox';
import { SpaceMark } from '@/ui/icons';
import { Btn, Group, Round, TITLE_WEIGHT, Wide, mono, useTop } from '@/ui/kit';
import { QuietSpaces } from '@/ui/quietspaces';
import { Row, rowStyles, spaceName } from '@/ui/row';
import { SpaceChips } from '@/ui/spacechips';
import { type Arranged, type Item, agentsIn, arrange, connectionLine, foldLabel, headline, itemKey, namesComputer, offlineRows, runningShown, shelve, undoTo } from '@/ui/inbox';
import { useTightWindows } from '@/ui/quotaLine';
import { quotaChips } from '@/ui/parse';
import { QuotaStrip } from '@/ui/quotastrip';
import { TabBar } from '@/ui/tabs';
import { statusColor } from '@/ui/status';
import { Swipe, justSwiped } from '@/ui/swipe';
import { TrustAsk } from '@/ui/trust';
import { type Theme, useTheme } from '@/ui/theme';
import { useHold, useSettled } from '@/ui/useHold';

const open = (m: Machine, c: Card) => {
  if (!justSwiped()) router.push({ pathname: '/chat', params: { machine: m.id, terminal: c.id } });
};

/** Long enough for the refresh an answer asks for to come back. */
const ANSWER_SHOWS_MS = 1000;

function run(p: Promise<unknown>) {
  p.catch((e) => oops((e as Error).message));
}

// What the row's buttons answer: a new command, or the row moving, and a tap waits again.
function ApprovalRow({ m, c, t, place, named }: Item & { t: Theme; place: string; named: boolean }) {
  const online = m.status === 'online';
  const ask = useAsk(m.id, c, online);
  const can = supports(m, 'answer');
  const settled = useSettled(`${ask.subject}|${ask.note}|${place}`);
  // Until the list shows the answer, the row is what it was: no second answer for what comes next.
  const [busy, setBusy] = useState(false);
  const off = busy || !settled.ready;
  const answer = (f: () => Promise<unknown>) => {
    if (busy || !settled.passes()) return;
    setBusy(true);
    f().then(
      (sent) => setTimeout(() => setBusy(false), sent === false ? 0 : ANSWER_SHOWS_MS),
      (e) => {
        setBusy(false);
        oops((e as Error).message);
      },
    );
  };
  return (
    <Row m={m} c={c} named={named} boxed onPress={() => open(m, c)}>
      <Text numberOfLines={3} style={[s.cmd, { backgroundColor: t.well, color: t.text }]}>
        {ask.subject}
      </Text>
      {!!ask.note && (
        <Text numberOfLines={1} style={{ color: t.dim, fontSize: 13 }}>
          {ask.note}
        </Text>
      )}
      {/* While off, a tap here stops here: it does not fall through and open the conversation. */}
      <View style={s.act} onStartShouldSetResponder={() => off}>
        {can && (
          <>
            <Btn primary icon="face" label="允许" disabled={off} onPress={() => answer(() => approve(m.id, c, ask))} />
            <Btn label="拒绝" disabled={off} onPress={() => answer(() => deny(m.id, c, ask))} />
          </>
        )}
        <Btn
          label="详情"
          onPress={() => router.push({ pathname: '/approve', params: { machine: m.id, terminal: c.id } })}
        />
      </View>
    </Row>
  );
}

type Settler = (m: Machine, c: Card, outcome: 'done' | 'deferred') => void;

function Swiped({ m, c, t, last, nth, named, boxed, onSettle }: Item & { t: Theme; last?: boolean; nth?: string; named: boolean; boxed?: boolean; onSettle: Settler }) {
  const can = supports(m, 'outcome') && !!c.session;
  const online = m.status === 'online';
  const buttons = online && Boolean(c.trust || c.approval);
  // Which card it is, where it sits and how tall: its buttons are somewhere else if any changes.
  // The count is known at once; the layout comes a moment later, and only native reports a move.
  const [laid, setLaid] = useState('');
  const place = `${nth}/${laid}`;
  const body = c.trust && online ? (
    <Row m={m} c={c} named={named} boxed onPress={() => open(m, c)}>
      <TrustAsk machine={m.id} card={c} can={supports(m, 'trust')} place={place} />
    </Row>
  ) : c.approval && online ? <ApprovalRow m={m} c={c} t={t} place={place} named={named} /> : <Row m={m} c={c} last={last} named={named} boxed={boxed} onPress={() => open(m, c)} />;
  return (
    <View
      onLayout={(e) => {
        const { y, height } = e.nativeEvent.layout;
        if (buttons) setLaid(`${y}/${height}`);
      }}
    >
      {can ? (
        <Swipe
          left={{ label: '已处理', color: t.okBg, fg: t.ok, run: () => onSettle(m, c, 'done') }}
          right={{ label: '稍后提醒', color: t.waitBg, fg: t.wait, run: () => onSettle(m, c, 'deferred') }}
        >
          {body}
        </Swipe>
      ) : (
        body
      )}
    </View>
  );
}

function Offline({ m, t }: { m: Machine; t: Theme }) {
  return (
    <Pressable onPress={() => reconnect(m.id)} style={[s.offline, { borderColor: t.sep }]}>
      <View style={[s.dotSmall, { backgroundColor: m.status === 'refused' ? t.err : t.dim }]} />
      <Text style={{ color: t.text, fontSize: 13 }}>{m.name || '电脑'}</Text>
      <Text numberOfLines={2} style={{ color: m.status === 'refused' ? t.err : t.dim, fontSize: 13, flex: 1 }}>
        {m.status === 'connecting' ? '正在连接…' : `${m.error ?? '没连上'} · 点这里重试`}
      </Text>
    </Pressable>
  );
}

/** How long 撤销 stays. */
const UNDO_MS = 7000;

const UNDO_WORDS = { done: '已标记「已处理」', deferred: '一小时后再提醒你' } as const;

export default function Inbox() {
  const t = useTheme();
  const top = useTop();
  const { loaded, machines } = useHub();
  const unsent = useOutbox();
  const [showLater, setShowLater] = useState(false);
  const [showDone, setShowDone] = useState(false);
  // Nothing moves under a finger or a scroll: what changed shows when it lifts or settles.
  const hold = useHold();
  const shown = useRef<Arranged | null>(null);
  if (!shown.current || !hold.held()) shown.current = arrange(machines, shown.current?.pins ?? []);
  const view = useInboxView();
  const shelf = shelve(shown.current, view.space);
  const { pinned, ahead, review, running, behind, later, done } = shelf;
  const named = namesComputer(machines);
  const line = connectionLine(machines);
  const offline = offlineRows(machines);
  const tight = useTightWindows(machines);
  const runningOpen = view.running !== false;
  const [allRunning, setAllRunning] = useState(false);
  const [undo, setUndo] = useState<{ text: string; back: () => void } | null>(null);
  const undoTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(undoTimer.current), []);
  const onSettle: Settler = (m, c, outcome) => {
    const before = undoTo(c);
    settle(m.id, c, outcome).then(() => {
      clearTimeout(undoTimer.current);
      setUndo({ text: UNDO_WORDS[outcome], back: () => run(settle(m.id, c, before)) });
      undoTimer.current = setTimeout(() => setUndo(null), UNDO_MS);
    }, (e) => oops((e as Error).message));
  };
  const where = (id: string, terminal: string) => {
    const m = machines.find((x) => x.id === id);
    const c = m?.cards.find((x) => x.id === terminal);
    return { m, c };
  };

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <View style={[s.hd, { paddingTop: top + 9 }]}>
        <View style={{ flex: 1, minWidth: 0 }}>
          <Text style={[s.title, { color: t.text }]} numberOfLines={1} adjustsFontSizeToFit minimumFontScale={0.75}>
            {headline(shelf, machines.length)}
          </Text>
          {!!line.text && (
            <Pressable accessibilityRole="button" hitSlop={8} onPress={() => router.push('/me')} style={s.line}>
              <View style={[s.dotSmall, { backgroundColor: machines.some((m) => m.status === 'online') ? t.ok : t.dim }]} />
              <Text numberOfLines={1} style={{ fontSize: 13, color: line.alert ? t.err : t.dim, flexShrink: 1 }}>
                {line.text}
              </Text>
            </Pressable>
          )}
        </View>
        <Round icon="search" label="搜索" onPress={() => router.push('/search')} />
        <Round icon="plus" label="新任务" onPress={() => router.push('/new')} />
      </View>
      <QuotaStrip chips={quotaChips(tight, shelf.filter === null ? null : agentsIn(shelf))} named={named} />
      <SpaceChips chips={shelf.chips} filter={shelf.filter} onPick={(space) => setInboxView({ space })} />
      <View style={{ flex: 1 }} {...hold.touch}>
      <ScrollView
        contentContainerStyle={{ paddingTop: 4, paddingBottom: undo ? 72 : 16 }}
        onScroll={hold.onScroll}
        scrollEventThrottle={32}
        keyboardShouldPersistTaps="handled"
      >
        {loaded && machines.length === 0 && (
          <View style={s.empty}>
            <Text style={[s.emptyTitle, { color: t.text }]}>还没有配对电脑</Text>
            <Text style={[s.emptyText, { color: t.dim }]}>
              在电脑的 Fibo 里点标题栏的手机按钮，选「配对新手机…」，再用这里扫码。
            </Text>
            <View style={{ alignSelf: 'stretch' }}>
              <Wide label="扫码配对" onPress={() => router.push('/pair')} />
            </View>
          </View>
        )}
        {/* What asks you first: a computer coming and going, or a group opening, pushes nothing you could tap. */}
        {pinned.map((i, n) => (
          <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} nth={String(n)} named={named} boxed onSettle={onSettle} />
        ))}
        {offline.map((m) => (
          <Offline key={m.id} m={m} t={t} />
        ))}
        {shelf.filter !== null && shelf.elsewhere > 0 && (
          <Pressable accessibilityRole="button" onPress={() => setInboxView({ space: null })} style={({ pressed }) => [s.elsewhere, { borderColor: t.sep }, pressed && { backgroundColor: t.well }]}>
            <Text style={{ fontSize: 14, color: t.wait, flex: 1 }}>{`其他空间还有 ${shelf.elsewhere} 件等你`}</Text>
            <Text style={{ fontSize: 13, color: t.dim }}>看全部</Text>
          </Pressable>
        )}
        {ahead.map((i) => (
          <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} named={named} onSettle={onSettle} />
        ))}
        {/* 等你验收 above 在跑: it wants you to look, 在跑 wants nothing (2026-11-08, docs/decisions/2026-11-08-review-above-running.md). */}
        {review.length > 0 && (
          <Group label={foldLabel(review)} color={statusColor(review[0].c, t)} open={view.review} onPress={() => setInboxView({ review: !view.review })} />
        )}
        {view.review && review.map((i) => <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} named={named} onSettle={onSettle} />)}
        {running.length > 0 && (
          <Group label={foldLabel(running)} color={statusColor(running[0].c, t)} open={runningOpen} onPress={() => setInboxView({ running: !runningOpen })} />
        )}
        {runningOpen && runningShown(running, allRunning).map((i) => <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} named={named} onSettle={onSettle} />)}
        {runningOpen && !allRunning && running.length > runningShown(running, false).length && (
          <Pressable accessibilityRole="button" onPress={() => setAllRunning(true)} style={({ pressed }) => [s.more, { borderColor: t.sep }, pressed && { backgroundColor: t.well }]}>
            <Text style={{ fontSize: 14, color: t.dim }}>{`看全部 ${running.length} 个`}</Text>
          </Pressable>
        )}
        {behind.map((i) => (
          <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} named={named} onSettle={onSettle} />
        ))}
        {loaded && machines.some((m) => m.status === 'online') && shelf.empty && (
          <Text style={[s.emptyText, { color: t.dim, paddingVertical: 24 }]}>{shelf.filter === null ? '没有要处理的事' : '这个空间没有要处理的事'}</Text>
        )}
        {unsent.length > 0 && <Group label="未发送" />}
        {unsent.map((u, i) => {
          const { m, c } = where(u.machine, u.terminal);
          const space = m && c ? spaceName(m, c) : '';
          return (
            <Swipe key={u.id} left={{ label: '删除', color: t.errBg, fg: t.err, run: () => drop(u.id) }}>
              <Pressable
                onPress={() => router.push({ pathname: '/chat', params: { machine: u.machine, terminal: u.terminal } })}
                style={[s.unsent, { borderColor: t.sep, backgroundColor: t.bg }, i === unsent.length - 1 && { borderBottomWidth: 0 }]}
              >
                <View style={rowStyles.l1}>
                  <SpaceMark name={space || '?'} id={c?.space || space || u.machine} />
                  <Text numberOfLines={1} style={[rowStyles.where, { color: t.dim }]}>
                    {[space, m?.name].filter(Boolean).join(' · ') || '电脑'}
                  </Text>
                  <Text style={[rowStyles.state, { color: u.error ? t.err : t.dim }]}>{u.error ? '没发出去' : '重连后发送'}</Text>
                </View>
                <Text numberOfLines={2} style={[rowStyles.title, { color: t.dim }]}>
                  {u.text}
                </Text>
                {!!u.error && <Text style={{ color: t.err, fontSize: 12 }}>{u.error}</Text>}
              </Pressable>
            </Swipe>
          );
        })}
        {later.length > 0 && <Group label={`稍后提醒 · ${later.length}`} open={showLater} onPress={() => setShowLater(!showLater)} />}
        {showLater &&
          later.map(({ m, c }) => (
            <Swipe key={`${m.id}/${c.id}`} left={{ label: '已处理', color: t.okBg, fg: t.ok, run: () => onSettle(m, c, 'done') }}>
              <Row m={m} c={c} named={named} dim right={{ text: '稍后提醒' }} onPress={() => open(m, c)} />
            </Swipe>
          ))}
        {done.length > 0 && <Group label={`已处理 · ${done.length}`} open={showDone} onPress={() => setShowDone(!showDone)} />}
        {showDone &&
          done.map(({ m, c }) => (
            <Swipe key={`${m.id}/${c.id}`} right={{ label: '放回收件箱', color: t.well, fg: t.text, run: () => run(settle(m.id, c, 'open')) }}>
              <Row m={m} c={c} named={named} dim right={{ text: '已处理' }} onPress={() => open(m, c)} />
            </Swipe>
          ))}
        <QuietSpaces machines={machines} open={view.quiet} onToggle={() => setInboxView({ quiet: !view.quiet })} named={named} />
      </ScrollView>
      {undo && (
        <View style={[s.undo, { backgroundColor: t.act }]}>
          <Text numberOfLines={1} style={{ fontSize: 14, color: t.onAct, flexShrink: 1 }}>
            {undo.text}
          </Text>
          <Pressable
            accessibilityRole="button"
            hitSlop={8}
            onPress={() => {
              undo.back();
              clearTimeout(undoTimer.current);
              setUndo(null);
            }}
            style={s.undoGo}
          >
            <Text style={{ fontSize: 14, fontWeight: '600', color: t.onAct }}>撤销</Text>
          </Pressable>
        </View>
      )}
      </View>
      <TabBar on="home" />
    </View>
  );
}

const s = StyleSheet.create({
  hd: { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 18, paddingBottom: 10, gap: 8 },
  elsewhere: { flexDirection: 'row', alignItems: 'center', gap: 10, minHeight: 44, paddingHorizontal: 18, borderBottomWidth: 1 },
  title: { fontSize: 26, lineHeight: 32, fontWeight: '600', letterSpacing: -0.3 },
  line: { flexDirection: 'row', alignItems: 'center', gap: 6, marginTop: 2 },
  more: { minHeight: 44, alignItems: 'center', justifyContent: 'center', borderBottomWidth: 1 },
  undo: { position: 'absolute', left: 12, right: 12, bottom: 8, minHeight: 48, borderRadius: 12, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 10, paddingLeft: 14, paddingRight: 4 },
  undoGo: { minHeight: 44, minWidth: 64, alignItems: 'center', justifyContent: 'center' },
  cmd: { fontFamily: mono, fontSize: 13, borderRadius: 10, paddingVertical: 9, paddingHorizontal: 11, marginTop: 6, overflow: 'hidden' },
  act: { flexDirection: 'row', gap: 8, marginTop: 8 },
  offline: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 18, paddingVertical: 10, borderBottomWidth: 1 },
  dotSmall: { width: 8, height: 8, borderRadius: 4 },
  unsent: { gap: 3, paddingVertical: 12, paddingHorizontal: 18, borderBottomWidth: 1 },
  empty: { alignItems: 'center', gap: 12, paddingTop: 80, paddingHorizontal: 24 },
  emptyTitle: { fontSize: 18, fontWeight: TITLE_WEIGHT },
  emptyText: { fontSize: 14, textAlign: 'center', lineHeight: 20 },
});
