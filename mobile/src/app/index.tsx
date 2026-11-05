// The inbox: every paired computer's conversations as flat rows, what needs you first.
// Approvals are answered in the row; swipe left for 已处理, right for 稍后提醒. What you
// wrote while a computer was away waits under 「未发送」.
import { router } from 'expo-router';
import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { approve, deny, oops, settle } from '@/actions';
import { type Card, type Machine, reconnect, supports, useHub } from '@/remote/hub';
import { useAsk } from '@/remote/useAsk';
import { drop, useOutbox } from '@/store/outbox';
import { SpaceMark } from '@/ui/icons';
import { Btn, Group, Pill, Round, TITLE_WEIGHT, Wide, mono, useBottom, useTop } from '@/ui/kit';
import { Row, rowStyles, spaceName } from '@/ui/row';
import { rank } from '@/ui/status';
import { Swipe, justSwiped } from '@/ui/swipe';
import { type Theme, useTheme } from '@/ui/theme';

const open = (m: Machine, c: Card) => {
  if (!justSwiped()) router.push({ pathname: '/chat', params: { machine: m.id, terminal: c.id } });
};

type Item = { m: Machine; c: Card };

/** Conversations sorted into the inbox, 稍后提醒 and 已处理. */
function sort(machines: Machine[]) {
  const inbox: Item[] = [];
  const later: Item[] = [];
  const done: Item[] = [];
  for (const m of machines) {
    for (const c of m.cards) {
      if (!c.agent || c.phase === 'shell') continue;
      const item = { m, c };
      // Something new to answer brings a settled conversation back.
      const asks = Boolean(c.approval || c.phase === 'waiting');
      if (c.outcome === 'done' && !asks) done.push(item);
      else if (c.outcome === 'deferred' && !asks) later.push(item);
      else inbox.push(item);
    }
  }
  const newest = (a: Item, b: Item) => (b.c.updated ?? 0) - (a.c.updated ?? 0);
  inbox.sort((a, b) => rank(a.c) - rank(b.c) || newest(a, b));
  later.sort(newest);
  done.sort(newest);
  return { inbox, later, done };
}

function run(p: Promise<unknown>) {
  p.catch((e) => oops((e as Error).message));
}

function ApprovalRow({ m, c, t }: Item & { t: Theme }) {
  const online = m.status === 'online';
  const ask = useAsk(m.id, c, online);
  const can = supports(m, 'answer');
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
      <View style={s.act}>
        {can && (
          <>
            <Btn primary icon="face" label="允许" onPress={() => run(approve(m.id, c, ask))} />
            <Btn label="拒绝" onPress={() => run(deny(m.id, c))} />
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

function Swiped({ m, c, t, last }: Item & { t: Theme; last?: boolean }) {
  const can = supports(m, 'outcome') && !!c.session;
  const body = c.approval && m.status === 'online' ? <ApprovalRow m={m} c={c} t={t} /> : <Row m={m} c={c} last={last} onPress={() => open(m, c)} />;
  if (!can) return body;
  return (
    <Swipe
      left={{ label: '已处理', color: t.okBg, fg: t.ok, run: () => run(settle(m.id, c, 'done')) }}
      right={{ label: '稍后提醒', color: t.waitBg, fg: t.wait, run: () => run(settle(m.id, c, 'deferred')) }}
    >
      {body}
    </Swipe>
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

export default function Inbox() {
  const t = useTheme();
  const top = useTop();
  const bottom = useBottom();
  const { loaded, machines } = useHub();
  const unsent = useOutbox();
  const [showLater, setShowLater] = useState(false);
  const [showDone, setShowDone] = useState(false);
  const { inbox, later, done } = sort(machines);
  const where = (id: string, terminal: string) => {
    const m = machines.find((x) => x.id === id);
    const c = m?.cards.find((x) => x.id === terminal);
    return { m, c };
  };

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <View style={[s.hd, { paddingTop: top + 9 }]}>
        <Text style={[s.brand, { color: t.text }]} numberOfLines={1}>
          Mooring
          {machines.length > 0 && <Text style={[s.count, { color: t.dim }]}>{`  ${machines.length} 台电脑`}</Text>}
        </Text>
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
      <ScrollView contentContainerStyle={{ paddingBottom: bottom + 16 }}>
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
        {machines.filter((m) => m.status !== 'online').map((m) => (
          <Offline key={m.id} m={m} t={t} />
        ))}
        {inbox.map(({ m, c }, i) => (
          <Swiped key={`${m.id}/${c.id}`} m={m} c={c} t={t} last={i === inbox.length - 1 && !unsent.length} />
        ))}
        {loaded && machines.some((m) => m.status === 'online') && inbox.length === 0 && (
          <Text style={[s.emptyText, { color: t.dim, paddingVertical: 24 }]}>没有要处理的事</Text>
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
  );
}

const s = StyleSheet.create({
  hd: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingHorizontal: 18, paddingBottom: 8, gap: 8 },
  brand: { fontSize: 22, fontWeight: '500', letterSpacing: -0.2, flexShrink: 1 },
  count: { fontSize: 10, letterSpacing: 1.2, fontWeight: '400' },
  cmd: { fontFamily: mono, fontSize: 13, borderRadius: 10, paddingVertical: 9, paddingHorizontal: 11, marginTop: 6, overflow: 'hidden' },
  act: { flexDirection: 'row', gap: 8, marginTop: 8 },
  offline: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 18, paddingVertical: 10, borderBottomWidth: 1 },
  dotSmall: { width: 8, height: 8, borderRadius: 4 },
  unsent: { gap: 3, paddingVertical: 12, paddingHorizontal: 18, borderBottomWidth: 1 },
  empty: { alignItems: 'center', gap: 12, paddingTop: 80, paddingHorizontal: 24 },
  emptyTitle: { fontSize: 18, fontWeight: TITLE_WEIGHT },
  emptyText: { fontSize: 14, textAlign: 'center', lineHeight: 20 },
});
