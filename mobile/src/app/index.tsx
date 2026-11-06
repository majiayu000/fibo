// The inbox: every paired computer's conversations as flat rows, what needs you first.
// Approvals are answered in the row; swipe left for 已处理, right for 稍后提醒. What you
// wrote while a computer was away waits under 「未发送」. Finished work folds into one line
// and a filter narrows to one space; the title keeps the count of what still waits for you.
import { router } from 'expo-router';
import { type ComponentRef, useRef, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { approve, deny, oops, settle } from '@/actions';
import { type Card, type Machine, reconnect, supports, useHub } from '@/remote/hub';
import { useAsk } from '@/remote/useAsk';
import { setInboxView, useInboxView } from '@/store/inboxView';
import { drop, useOutbox } from '@/store/outbox';
import { SpaceMark } from '@/ui/icons';
import { Btn, Group, Pill, Round, TITLE_WEIGHT, Wide, mono, useBottom, useTop } from '@/ui/kit';
import { Row, rowStyles, spaceName } from '@/ui/row';
import { type Arranged, type Chip, type Item, arrange, foldLabel, itemKey, shelve } from '@/ui/inbox';
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
function ApprovalRow({ m, c, t, place }: Item & { t: Theme; place: string }) {
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
    <Row m={m} c={c} onPress={() => open(m, c)}>
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

function Swiped({ m, c, t, last, nth }: Item & { t: Theme; last?: boolean; nth?: string }) {
  const can = supports(m, 'outcome') && !!c.session;
  const online = m.status === 'online';
  const buttons = online && Boolean(c.trust || c.approval);
  // Which card it is, where it sits and how tall: its buttons are somewhere else if any changes.
  // The count is known at once; the layout comes a moment later, and only native reports a move.
  const [laid, setLaid] = useState('');
  const place = `${nth}/${laid}`;
  const body = c.trust && online ? (
    <Row m={m} c={c} onPress={() => open(m, c)}>
      <TrustAsk machine={m.id} card={c} can={supports(m, 'trust')} place={place} />
    </Row>
  ) : c.approval && online ? <ApprovalRow m={m} c={c} t={t} place={place} /> : <Row m={m} c={c} last={last} onPress={() => open(m, c)} />;
  return (
    <View
      onLayout={(e) => {
        const { y, height } = e.nativeEvent.layout;
        if (buttons) setLaid(`${y}/${height}`);
      }}
    >
      {can ? (
        <Swipe
          left={{ label: '已处理', color: t.okBg, fg: t.ok, run: () => run(settle(m.id, c, 'done')) }}
          right={{ label: '稍后提醒', color: t.waitBg, fg: t.wait, run: () => run(settle(m.id, c, 'deferred')) }}
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

function SpaceChip({ chip, on, t, onPress }: { chip: Chip; on: boolean; t: Theme; onPress: () => void }) {
  const label = chip.name ?? '全部';
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected: on }}
      accessibilityLabel={chip.acts > 0 ? `${label}，${chip.count} 个，${chip.acts} 件等你` : `${label}，${chip.count} 个`}
      hitSlop={{ top: 7, bottom: 7 }}
      onPress={onPress}
      style={({ pressed }) => [s.chip, { borderColor: on ? t.act : t.line, backgroundColor: on ? t.act : t.card }, pressed && { opacity: 0.7 }]}
    >
      <Text numberOfLines={1} style={[s.chipText, { color: on ? t.onAct : t.text }]}>
        {label}
      </Text>
      {chip.count > 0 && <Text style={[s.chipText, chip.acts > 0 && !on ? { color: t.wait, fontWeight: '600' } : { color: on ? t.onAct : t.dim }]}>{chip.count}</Text>}
    </Pressable>
  );
}

export default function Inbox() {
  const t = useTheme();
  const top = useTop();
  const bottom = useBottom();
  const { loaded, machines } = useHub();
  const unsent = useOutbox();
  const [showLater, setShowLater] = useState(false);
  const [showDone, setShowDone] = useState(false);
  // Nothing moves under a finger or a scroll: what changed shows when it lifts or settles.
  const hold = useHold();
  const shown = useRef<Arranged | null>(null);
  if (!shown.current || !hold.held()) shown.current = arrange(machines, shown.current?.pins ?? []);
  const { offline } = shown.current;
  const view = useInboxView();
  const shelf = shelve(shown.current, view.space);
  const { pinned, ahead, review, running, behind, later, done, chips, filter } = shelf;
  const list = useRef<ComponentRef<typeof ScrollView>>(null);
  const pick = (space: string | null) => {
    setInboxView({ space });
    list.current?.scrollTo({ y: 0, animated: false });
  };
  // Only the last row of all drops its hairline; `after` says something still follows this group.
  const tail = (n: number, length: number, after: boolean) => n === length - 1 && !after && !unsent.length;
  const where = (id: string, terminal: string) => {
    const m = machines.find((x) => x.id === id);
    const c = m?.cards.find((x) => x.id === terminal);
    return { m, c };
  };

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <View style={[s.hd, { paddingTop: top + 9 }]}>
        {/* Stacked: the count must not be cut on a narrow phone, and the buttons set the height either way. */}
        <View style={{ flexShrink: 1 }}>
          <Text style={[s.brand, { color: t.text }]} numberOfLines={1}>
            Mooring
          </Text>
          {shelf.waiting > 0 ? (
            <Text numberOfLines={1} style={[s.waiting, { color: shelf.acts > 0 ? t.wait : t.ok }]}>{`${shelf.waiting} 件等你`}</Text>
          ) : (
            machines.length > 0 && <Text numberOfLines={1} style={[s.count, { color: t.dim }]}>{`${machines.length} 台电脑`}</Text>
          )}
        </View>
        <View style={{ flexDirection: 'row', gap: 8 }}>
          <Pill
            items={[
              { icon: 'board', label: '看板', onPress: () => router.push('/board'), width: 36 },
              { icon: 'computer', label: '电脑', onPress: () => router.push('/machines'), width: 36 },
              { icon: 'quota', label: '额度', onPress: () => router.push('/quota'), width: 36 },
              { icon: 'gear', label: '设置', onPress: () => router.push('/settings'), width: 36 },
            ]}
          />
          <Round icon="plus" label="新任务" onPress={() => router.push('/new')} />
        </View>
      </View>
      {chips.length > 0 && (
        <ScrollView horizontal showsHorizontalScrollIndicator={false} style={{ flexGrow: 0 }} contentContainerStyle={s.chips}>
          {chips.map((chip) => (
            <SpaceChip key={chip.name ?? ''} chip={chip} on={chip.name === filter} t={t} onPress={() => pick(chip.name)} />
          ))}
        </ScrollView>
      )}
      <View style={{ flex: 1 }} {...hold.touch}>
      <ScrollView
        ref={list}
        contentContainerStyle={{ paddingBottom: bottom + 16 }}
        onScroll={hold.onScroll}
        scrollEventThrottle={32}
        keyboardShouldPersistTaps="handled"
      >
        {loaded && machines.length === 0 && (
          <View style={s.empty}>
            <Text style={[s.emptyTitle, { color: t.text }]}>还没有配对电脑</Text>
            <Text style={[s.emptyText, { color: t.dim }]}>
              在电脑的 Mooring 里点标题栏的手机按钮，选「配对新手机…」，再用这里扫码。
            </Text>
            <View style={{ alignSelf: 'stretch' }}>
              <Wide label="扫码配对" onPress={() => router.push('/pair')} />
            </View>
          </View>
        )}
        {pinned.map((i, n) => (
          // The filter row coming or going moves the whole list: its buttons wait again too.
          <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} nth={`${chips.length > 0}/${n}`} />
        ))}
        {/* Under what asks you, like the bar below: an approval arriving elsewhere pushes nothing you could tap. */}
        {shelf.elsewhere > 0 && (
          <Pressable accessibilityRole="button" onPress={() => pick(null)} style={({ pressed }) => [s.elsewhere, { backgroundColor: t.waitBg }, pressed && { opacity: 0.7 }]}>
            <Text style={[s.elsewhereText, { color: t.wait }]}>{`其他空间还有 ${shelf.elsewhere} 件等你`}</Text>
            <Text style={[s.elsewhereGo, { color: t.wait }]}>看全部</Text>
          </Pressable>
        )}
        {/* Under what asks you: a computer coming and going pushes nothing you could tap. */}
        {offline.map((m) => (
          <Offline key={m.id} m={m} t={t} />
        ))}
        {ahead.map((i, n) => (
          <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} last={tail(n, ahead.length, review.length + running.length + behind.length > 0)} />
        ))}
        {review.length > 0 && (
          <Group label={foldLabel(review)} color={statusColor(review[0].c, t)} open={view.review} onPress={() => setInboxView({ review: !view.review })} />
        )}
        {view.review && review.map((i, n) => <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} last={tail(n, review.length, running.length + behind.length > 0)} />)}
        {running.map((i, n) => (
          <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} last={tail(n, running.length, behind.length > 0)} />
        ))}
        {behind.map((i, n) => (
          <Swiped key={itemKey(i)} m={i.m} c={i.c} t={t} last={tail(n, behind.length, false)} />
        ))}
        {loaded && machines.some((m) => m.status === 'online') && shelf.empty && (
          <Text style={[s.emptyText, { color: t.dim, paddingVertical: 24 }]}>{filter === null ? '没有要处理的事' : '这个空间没有要处理的事'}</Text>
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
            <Swipe key={`${m.id}/${c.id}`} left={{ label: '已处理', color: t.okBg, fg: t.ok, run: () => run(settle(m.id, c, 'done')) }}>
              <Row m={m} c={c} dim right={{ text: '稍后提醒' }} onPress={() => open(m, c)} />
            </Swipe>
          ))}
        {done.length > 0 && <Group label={`已处理 · ${done.length}`} open={showDone} onPress={() => setShowDone(!showDone)} />}
        {showDone &&
          done.map(({ m, c }) => (
            <Swipe key={`${m.id}/${c.id}`} right={{ label: '放回收件箱', color: t.well, fg: t.text, run: () => run(settle(m.id, c, 'open')) }}>
              <Row m={m} c={c} dim right={{ text: '已处理' }} onPress={() => open(m, c)} />
            </Swipe>
          ))}
      </ScrollView>
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  hd: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingHorizontal: 18, paddingBottom: 8, gap: 8 },
  brand: { fontSize: 22, lineHeight: 26, fontWeight: '500', letterSpacing: -0.2 },
  count: { fontSize: 10, lineHeight: 15, letterSpacing: 1.2, fontWeight: '400' },
  waiting: { fontSize: 12, lineHeight: 15, fontWeight: '500' },
  chips: { flexDirection: 'row', alignItems: 'center', gap: 6, paddingHorizontal: 18, paddingBottom: 8 },
  chip: { flexDirection: 'row', alignItems: 'center', gap: 5, height: 30, borderRadius: 15, borderWidth: 1, paddingHorizontal: 11, maxWidth: 180 },
  chipText: { fontSize: 13, flexShrink: 1 },
  elsewhere: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 10, minHeight: 44, paddingHorizontal: 18, paddingVertical: 10 },
  elsewhereText: { fontSize: 14, fontWeight: '600', flexShrink: 1 },
  elsewhereGo: { fontSize: 13 },
  cmd: { fontFamily: mono, fontSize: 13, borderRadius: 10, paddingVertical: 9, paddingHorizontal: 11, marginTop: 6, overflow: 'hidden' },
  act: { flexDirection: 'row', gap: 8, marginTop: 8 },
  offline: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 18, paddingVertical: 10, borderBottomWidth: 1 },
  dotSmall: { width: 8, height: 8, borderRadius: 4 },
  unsent: { gap: 3, paddingVertical: 12, paddingHorizontal: 18, borderBottomWidth: 1 },
  empty: { alignItems: 'center', gap: 12, paddingTop: 80, paddingHorizontal: 24 },
  emptyTitle: { fontSize: 18, fontWeight: TITLE_WEIGHT },
  emptyText: { fontSize: 14, textAlign: 'center', lineHeight: 20 },
});
