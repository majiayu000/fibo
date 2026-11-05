// One conversation: your words in grey bubbles, the Agent's flat, its work folded into a
// line that opens each call it made; the approval or question it waits on; a composer card whose
// send key turns into stop while the Agent runs.
import { router, useLocalSearchParams } from 'expo-router';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, FlatList, KeyboardAvoidingView, Platform, Pressable, Text, View } from 'react-native';
import { approve, deny, oops } from '@/actions';
import { catchUp } from '@/remote/gap';
import { refresh, remote, speaksV2, supports, useHub, write } from '@/remote/hub';
import { UNSURE } from '@/remote/named';
import { useAsk } from '@/remote/useAsk';
import { drop, enqueue, retry, useOutbox } from '@/store/outbox';
import { Composer } from '@/ui/composer';
import { cardTitle } from '@/ui/format';
import { Icon } from '@/ui/icons';
import { Btn, Nav, Notice, Pill, mono } from '@/ui/kit';
import { type Message, Mine, Theirs } from '@/ui/message';
import { choices, questionText, partialWords, readBackWords } from '@/ui/parse';
import { RunningWord, spaceName } from '@/ui/row';
import { TerminalSheet } from '@/ui/sheets';
import { agentName, statusColor } from '@/ui/status';
import { TrustAsk } from '@/ui/trust';
import { useTheme } from '@/ui/theme';
import { useSettled } from '@/ui/useHold';

type Pending = { text: string; at: number };

/** A reindexed log gives its messages new seqs; the same words at the same time are one. */
const key = (m: Message) => `${m.at}|${m.role}|${m.text}`;

function merge(have: Message[], got: Message[]): Message[] {
  const seen = new Set(have.map(key));
  const fresh = got.filter((m) => !seen.has(key(m)));
  return fresh.length ? [...have, ...fresh].sort((a, b) => a.seq - b.seq) : have;
}

const POLL_MS = 2000;
const NO_CONVERSATION = /没有 Agent 对话/;

type Line = { kind: 'msg'; m: Message } | { kind: 'pending'; p: Pending } | { kind: 'unsent'; id: string; text: string; error?: string };

export default function Chat() {
  const t = useTheme();
  const { machine, terminal } = useLocalSearchParams<{ machine: string; terminal: string }>();
  const { machines } = useHub();
  const m = machines.find((x) => x.id === machine);
  const card = m?.cards.find((c) => c.id === terminal);
  const online = m?.status === 'online';
  const tools = speaksV2(m);
  const outbox = useOutbox().filter((u) => u.machine === machine && u.terminal === terminal);

  const [messages, setMessages] = useState<Message[]>([]);
  const [session, setSession] = useState<string | null>(null);
  const [more, setMore] = useState(false);
  const [partial, setPartial] = useState(false);
  const [readingBack, setReadingBack] = useState(false);
  const [readFailed, setReadFailed] = useState('');
  const paging = useRef(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [pending, setPending] = useState<Pending[]>([]);
  const [term, setTerm] = useState(false);
  const last = useRef(0);
  last.current = messages.at(-1)?.seq ?? 0;

  // The newest page; again whenever the connection comes back, and when the terminal's
  // card names another conversation (a new one started in the same terminal).
  const shown = useRef<string | null>(null);
  const cardSession = card?.session ?? null;
  useEffect(() => {
    const r = online ? remote(machine) : null;
    if (!r) {
      setLoading(false);
      return;
    }
    let live = true;
    const extra = tools ? { tools: true } : {};
    r.request('chat', { terminal, limit: 50, ...extra })
      .then(async (page) => {
        if (!live) return;
        const same = shown.current === null || shown.current === page.session;
        const had = same ? last.current : 0;
        shown.current = page.session;
        setSession(page.session);
        setMore(page.more);
        setPartial(!!page.partial);
        // Another conversation: its messages replace the old one's, not join them.
        setMessages((have) => (same ? merge(have, page.messages) : page.messages));
        setError('');
        // Back after a while: what came between the last one seen and this page.
        const gap = await catchUp<Message>(
          (after, limit) => r.request('chat', { session: page.session, after, limit, ...extra }).then((p) => p.messages),
          had,
          page.messages[0]?.seq,
          page.more,
        );
        if (live && gap.length && shown.current === page.session) setMessages((have) => merge(have, gap));
      })
      // A shell whose Agent has not started has no conversation yet: the empty state says
      // so, and the card naming a session brings this effect back.
      .catch((e) => live && !NO_CONVERSATION.test(e.message) && setError(e.message))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [online, machine, terminal, cardSession, tools]);

  // New messages: the Agent's words mid-turn raise no event, so ask every two seconds.
  useEffect(() => {
    if (!session || !online) return;
    const timer = setInterval(async () => {
      const r = remote(machine);
      if (!r) return;
      try {
        const page = await r.request('chat', { session, after: last.current, ...(tools ? { tools: true } : {}) });
        // An answer for a conversation this page has since left is dropped.
        if (page.messages.length && shown.current === session) setMessages((have) => merge(have, page.messages));
      } catch {
        // The next tick or the reconnect tries again.
      }
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [session, online, machine, tools]);

  // A sent message shows until the log has it, or for half a minute.
  useEffect(() => {
    setPending((p) =>
      p.filter(
        (x) =>
          Date.now() / 1000 - x.at < 30 &&
          !messages.some((m) => m.role === 'user' && m.at >= x.at - 5 && m.text.trim() === x.text.trim()),
      ),
    );
  }, [messages]);

  const canReadBack = partial && !more && supports(m, 'backfill');
  // Scrolled to the top: the next page, or when the index ends, the next stretch of the log.
  const older = useCallback(async () => {
    const r = remote(machine);
    if (!r || !session || paging.current) return;
    if (!canReadBack && !(more && messages.length)) return;
    paging.current = true;
    const extra = tools ? { tools: true } : {};
    try {
      if (canReadBack) {
        setReadingBack(true);
        setReadFailed('');
        // Each call reads for at most about a second; a stretch of the log may hold no message.
        let reply;
        do {
          reply = await r.request('backfill', { session });
        } while (!reply.added && !reply.done && shown.current === session);
      }
      const before = messages[0] ? { before: messages[0].seq } : { limit: 50 };
      const page = await r.request('chat', { session, ...before, ...extra });
      if (shown.current !== session) return;
      setMore(page.more);
      setPartial(!!page.partial);
      setMessages((have) => merge(have, page.messages));
    } catch (e) {
      setReadFailed((e as Error).message);
    } finally {
      paging.current = false;
      setReadingBack(false);
    }
  }, [machine, session, more, messages, tools, canReadBack]);

  /** Sends `text`; while the computer is away it waits in the outbox instead. */
  async function send(text: string): Promise<boolean> {
    if (!online) {
      enqueue({ machine, terminal, text });
      return true;
    }
    setPending((p) => [...p, { text, at: Date.now() / 1000 }]);
    try {
      await write(machine, 'send', { terminal, text, enter: true });
      setError('');
      return true;
    } catch (e) {
      setPending((p) => p.filter((x) => x.text !== text));
      const why = (e as Error).message;
      setError(why === UNSURE ? why : `没发出去：${why}`);
      return false;
    }
  }

  async function act(op: string, fields: Record<string, unknown>) {
    try {
      await write(machine, op, { terminal, ...fields });
      setError('');
      setTimeout(() => refresh(machine), 300);
    } catch (e) {
      setError((e as Error).message);
    }
  }

  async function stop() {
    try {
      const done = await write(machine, 'stop', { terminal });
      setError(done?.closed_background ? '这一轮中断了。它后台没结束的命令也一起关了。' : '');
      setTimeout(() => refresh(machine), 300);
    } catch (e) {
      setError((e as Error).message);
    }
  }

  // Inverted list: index 0 sits at the bottom.
  const rows: Line[] = useMemo(
    () => [
      ...outbox.map((u) => ({ kind: 'unsent' as const, id: u.id, text: u.text, error: u.error })).reverse(),
      ...pending.map((p) => ({ kind: 'pending' as const, p })).reverse(),
      ...[...messages].reverse().map((m) => ({ kind: 'msg' as const, m })),
    ],
    [messages, pending, outbox],
  );

  const ask = useAsk(machine, card, online);
  const settled = useSettled(`${ask.subject}|${ask.note}`);
  const waiting = online && card?.phase === 'waiting' && !card.approval ? card.waiting : null;
  const options = choices(waiting);
  const agent = agentName(card?.agent);
  const running = online && card?.phase === 'working';
  const toDiff = useCallback(() => router.push({ pathname: '/diff', params: { machine, terminal } }), [machine, terminal]);
  const toTools = useCallback(
    (seq?: number) => router.push({ pathname: '/tools', params: { machine, terminal, session: shown.current ?? '', ...(seq ? { seq: String(seq) } : {}) } }),
    [machine, terminal],
  );
  // The hub's heartbeat redraws this screen; a row redraws only when its own message changes.
  const renderItem = useCallback(
    ({ item: l }: { item: Line }) =>
      l.kind === 'msg' ? (
        l.m.role === 'user' ? (
          <Mine text={l.m.text} at={l.m.at} copyable />
        ) : (
          <Theirs m={l.m} agent={agent} onTools={toTools} />
        )
      ) : l.kind === 'pending' ? (
        <Mine text={l.p.text} note="发送中…" tone="pending" />
      ) : (
        <Pressable onPress={() => l.error && retry(l.id)} onLongPress={() => drop(l.id)}>
          <Mine text={l.text} note={l.error ? `没发出去：${l.error}（点一下重发，按住删除）` : '重连后发送（按住删除）'} tone={l.error ? 'error' : 'pending'} />
        </Pressable>
      ),
    [agent, toTools],
  );

  return (
    <KeyboardAvoidingView style={{ flex: 1, backgroundColor: t.bg }} behavior={Platform.OS === 'web' ? undefined : 'padding'}>
      <Nav
        rule
        title={card ? cardTitle(card) : '对话'}
        sub={
          <View style={{ flexDirection: 'row', alignItems: 'baseline' }}>
            <Text numberOfLines={1} style={{ fontSize: 12, color: t.dim, flexShrink: 1 }}>
              {[m && card ? spaceName(m, card) : '', machines.length > 1 ? m?.name : '', card?.branch].filter(Boolean).join(' · ')}
            </Text>
            {card && online && (
              <>
                <Text style={{ fontSize: 12, color: t.dim }}> · </Text>
                <RunningWord c={card} color={statusColor(card, t)} style={{ fontSize: 12, flexShrink: 0 }} />
              </>
            )}
          </View>
        }
        right={
          <Pill
            items={[
              { icon: 'diff', label: '改动', onPress: toDiff },
              { icon: 'terminal', label: '终端最后几行', onPress: () => setTerm(true) },
            ]}
          />
        }
      />
      {!online && <Notice text={m?.status === 'connecting' ? '正在连接…' : `${m?.error ?? '电脑没连上'}；现在写的话会在重连后发送`} />}
      {loading && online ? (
        <ActivityIndicator style={{ marginTop: 40 }} />
      ) : (
        <FlatList
          inverted
          data={rows}
          keyExtractor={(l) => (l.kind === 'msg' ? `m${l.m.seq}` : l.kind === 'pending' ? `p${l.p.at}${l.p.text}` : `u${l.id}`)}
          renderItem={renderItem}
          initialNumToRender={8}
          maxToRenderPerBatch={6}
          windowSize={9}
          onEndReached={older}
          onEndReachedThreshold={0.3}
          contentContainerStyle={{ paddingHorizontal: 18, paddingVertical: 14, gap: 10 }}
          ListHeaderComponent={
            running && session && supports(m, 'toolcalls') ? (
              <Pressable accessibilityRole="button" onPress={() => toTools()} style={{ flexDirection: 'row', alignItems: 'center', paddingVertical: 8, gap: 6 }}>
                <Text style={{ flex: 1, fontSize: 13, color: t.dim }}>看它到现在做了什么</Text>
                <Icon name="right" size={14} width={2} color={t.dim} />
              </Pressable>
            ) : undefined
          }
          ListFooterComponent={
            canReadBack && messages.length > 0 ? (
              <Pressable accessibilityRole="button" disabled={readingBack} onPress={older} style={{ paddingVertical: 8 }}>
                <Text style={{ color: t.dim, fontSize: 13, lineHeight: 19, textAlign: 'center' }}>{readBackWords(readingBack, readFailed)}</Text>
              </Pressable>
            ) : partial && !more && messages.length > 0 ? (
              <Text style={{ color: t.dim, fontSize: 13, lineHeight: 19, textAlign: 'center', paddingVertical: 8 }}>{partialWords(messages.length)}</Text>
            ) : undefined
          }
          ListEmptyComponent={
            <Text onPress={canReadBack && !readingBack ? older : undefined} style={{ color: t.dim, textAlign: 'center' }}>
              {canReadBack ? readBackWords(readingBack, readFailed) : partial ? partialWords(0) : card?.session ? '这段对话还没有消息' : card?.trust ? '答完下面的问题，它才会开始' : card?.note || '这个终端里还没有 Agent 对话；Agent 起来后这里会接上'}
            </Text>
          }
        />
      )}
      {!!card?.approval && online && (
        <View style={{ marginHorizontal: 12, marginBottom: 10, padding: 14, gap: 10, borderRadius: 20, borderWidth: 1.5, borderColor: t.wait, backgroundColor: t.card }}>
          <Text style={{ color: t.wait, fontSize: 14, fontWeight: '500' }}>等你审批</Text>
          <Text numberOfLines={3} style={{ fontFamily: mono, fontSize: 13, color: t.text, backgroundColor: t.well, borderRadius: 10, padding: 10, overflow: 'hidden' }}>
            {ask.subject}
          </Text>
          {!!ask.note && (
            <Text numberOfLines={2} style={{ color: t.dim, fontSize: 13, lineHeight: 18 }}>
              {ask.note}
            </Text>
          )}
          <View style={{ flexDirection: 'row', gap: 8 }}>
            {supports(m, 'answer') && (
              <>
                <Btn primary icon="face" label="允许" disabled={!settled.ready} onPress={() => settled.passes() && approve(machine, card, ask).catch((e) => oops(e.message))} />
                <Btn label="拒绝" disabled={!settled.ready} onPress={() => settled.passes() && deny(machine, card, ask).catch((e) => oops(e.message))} />
              </>
            )}
            <Btn label="详情" onPress={() => router.push({ pathname: '/approve', params: { machine, terminal } })} />
          </View>
        </View>
      )}
      {!!card?.trust && online && (
        <View style={{ marginHorizontal: 12, marginBottom: 10, padding: 14, borderRadius: 20, borderWidth: 1.5, borderColor: t.wait, backgroundColor: t.card }}>
          <TrustAsk machine={machine} card={card} can={supports(m, 'trust')} />
        </View>
      )}
      {!!waiting && (
        <View style={{ paddingHorizontal: 18, paddingBottom: 10, gap: 8 }}>
          <Text numberOfLines={6} style={{ color: t.wait, fontSize: 13, lineHeight: 19 }}>
            {questionText(waiting) || '等你回答'}
          </Text>
          {supports(m, 'keys') && (
            <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
              {options.map((o) => (
                <Btn key={o.key} label={o.label} onPress={() => act('keys', { keys: [o.key] })} />
              ))}
              <Btn label="Esc" onPress={() => act('keys', { keys: ['esc'] })} />
            </View>
          )}
        </View>
      )}
      {!!error && <Notice tone="danger" text={error} />}
      {online && !card ? (
        <Notice text="这个终端已经关了" />
      ) : card?.trust && online ? null :  m?.role && m.role !== 'full' && !supports(m, 'send') ? (
        <Notice text={`这台手机的权限是「${m.role === 'view' ? '只看' : '能答审批'}」，不能发消息`} />
      ) : (
        <Composer
          agent={agent}
          model={card?.model}
          permission={card?.permission}
          running={!!running}
          canStop={supports(m, 'stop')}
          canContinue={online && card?.phase === 'idle'}
          offline={!online}
          onSend={send}
          onStop={() => stop()}
        />
      )}
      <TerminalSheet visible={term} machine={machine} terminal={terminal} onClose={() => setTerm(false)} />
    </KeyboardAvoidingView>
  );
}

