// One approval, large: what it wants to run in big mono type, what it said just before,
// 拒绝 and 允许 (Face ID when settings ask), the menu's own 「don't ask again」 option in its
// words (none when the menu has none), and 「拒绝，并告诉它…」.
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { ScrollView, StyleSheet, Text, View } from 'react-native';
import { approve, deny, oops } from '@/actions';
import { remote, supports, useHub, write } from '@/remote/hub';
import { useAsk } from '@/remote/useAsk';
import { useSettings } from '@/store/settings';
import { cardTitle } from '@/ui/format';
import { SpaceMark } from '@/ui/icons';
import { Nav, Notice, TITLE_WEIGHT, Wide, mono, useBottom } from '@/ui/kit';
import { spaceName } from '@/ui/row';
import { agentName } from '@/ui/status';
import { SayDialog } from '@/ui/sheets';
import { ago, useTheme } from '@/ui/theme';
import { useSettled } from '@/ui/useHold';
import { tr } from '@/i18n';

export default function Approve() {
  const t = useTheme();
  const bottom = useBottom();
  const { lock } = useSettings();
  const { machine, terminal } = useLocalSearchParams<{ machine: string; terminal: string }>();
  const { machines } = useHub();
  const m = machines.find((x) => x.id === machine);
  const card = m?.cards.find((c) => c.id === terminal);
  const online = m?.status === 'online';
  const ask = useAsk(machine, card, online);
  const [said, setSaid] = useState('');
  const [telling, setTelling] = useState(false);
  const [busy, setBusy] = useState(false);

  // What it said just before asking: its last message.
  useEffect(() => {
    const r = online ? remote(machine) : null;
    if (!r) return;
    let live = true;
    r.request('chat', { terminal, limit: 4 })
      .then((p) => {
        const last = [...(p.messages ?? [])].reverse().find((x: { role: string }) => x.role === 'agent');
        if (live && last) setSaid(String(last.text).trim().split('\n')[0].slice(0, 160));
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [online, machine, terminal]);

  // What it asks arrives after the page does; 允许 waits until that has been up a moment.
  const settled = useSettled(`${ask.subject}|${ask.note}|${ask.always}`);
  const off = busy || !settled.ready;
  const gone = !card?.approval;
  const can = supports(m, 'answer');
  const space = m && card ? spaceName(m, card) : '';

  async function run(f: () => Promise<unknown>) {
    if (busy || !settled.passes()) return;
    setBusy(true);
    try {
      const done = await f();
      if (done !== false) router.back();
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav />
      <ScrollView keyboardShouldPersistTaps="handled" contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 4, paddingBottom: bottom + 16, gap: 16 }}>
        <View style={s.head}>
          <SpaceMark name={space || 'M'} id={card?.space || space || machine} size={56} radius={16} />
          <View style={{ flex: 1, gap: 3 }}>
            <Text style={[s.title, { color: t.text }]} numberOfLines={2}>
              {card ? cardTitle(card) : tr('审批')}
            </Text>
            <Text style={{ fontSize: 15, color: gone ? t.dim : t.wait, fontWeight: '500' }}>{gone ? tr('已经不在等了') : tr('等你同意')}</Text>
            <Text style={{ fontSize: 13, color: t.dim }} numberOfLines={1}>
              {[m?.name, space, agentName(card?.agent), card?.updated ? ago(card.updated) : ''].filter(Boolean).join(' · ')}
            </Text>
          </View>
        </View>
        {!online && <Notice text={m?.error ?? tr('电脑没连上')} />}
        {card?.approval && (
          <View style={[s.box, { borderColor: t.dark ? 'rgba(240,136,62,0.45)' : 'rgba(187,77,0,0.35)', backgroundColor: t.card }]}>
            <Text style={{ fontSize: 14, color: t.wait, fontWeight: '500' }}>{/^edit|write|create/i.test(ask.kind) ? tr('它要改') : tr('它要运行')}</Text>
            {!!ask.note && (
              <Text selectable style={{ fontSize: 15, color: t.text, lineHeight: 22 }}>
                {ask.note}
              </Text>
            )}
            <Text selectable style={[s.cmd, { backgroundColor: t.well, color: t.text }]}>
              {ask.subject}
            </Text>
            {!!ask.detail && (
              <Text selectable style={[s.detail, { backgroundColor: t.well, color: t.dim }]}>
                {ask.detail}
              </Text>
            )}
            {ask.truncated && <Text style={{ fontSize: 13, color: t.wait }}>{tr('电脑只读到这些，上面还有。打开终端能看全。')}</Text>}
            {!!said && (
              <Text style={{ fontSize: 13, color: t.dim, lineHeight: 20 }}>
                {tr('它刚才说：「{said}」', { said })}
              </Text>
            )}
            {can ? (
              <View style={{ flexDirection: 'row', gap: 10 }}>
                <View style={{ flex: 1 }}>
                  <Wide label={tr('拒绝')} kind="danger" disabled={off} onPress={() => run(() => deny(machine, card, ask))} textStyle={{ fontSize: 17 }} />
                </View>
                <View style={{ flex: 1 }}>
                  <Wide
                    label={tr('允许')}
                    kind="allow"
                    icon="face"
                    disabled={off}
                    onPress={() => run(() => approve(machine, card, ask))}
                    textStyle={{ fontSize: 17 }}
                  />
                </View>
              </View>
            ) : (
              <Notice text={tr('这台手机的权限是「只看」，不能答审批')} />
            )}
            <Text style={{ fontSize: 12, color: t.dim }}>
              {lock ? tr('允许要 Face ID / 指纹。') : tr('允许不用验证；可以在设置里打开 Face ID。')}
            </Text>
          </View>
        )}
        {card?.approval && can && (
          <View style={{ gap: 8 }}>
            {!!ask.always && (
              <Wide
                kind="outline"
                height={46}
                wrap
                label={ask.always}
                disabled={off}
                onPress={() => run(() => approve(machine, card, ask, true))}
                textStyle={{ fontSize: 15, fontWeight: '500' }}
              />
            )}
            {supports(m, 'send') && (
              <Wide kind="outline" height={46} label={tr('拒绝，并告诉它…')} disabled={busy} onPress={() => setTelling(true)} textStyle={{ fontSize: 15, fontWeight: '500' }} />
            )}
          </View>
        )}
        {gone && card && <Notice text={tr('电脑上这个审批已经答过或者取消了。')} />}
      </ScrollView>
      <SayDialog
        visible={telling}
        title={tr('拒绝，并告诉它')}
        placeholder={tr('比如：先别推，等我回来看一眼')}
        action={tr('拒绝并发送')}
        onClose={() => setTelling(false)}
        onSend={(text) => {
          setTelling(false);
          if (!card) return;
          run(async () => {
            await deny(machine, card, ask);
            // The prompt closes first; then the sentence lands in the Agent's input.
            await new Promise((r) => setTimeout(r, 600));
            await write(machine, 'send', { terminal, text, enter: true });
          });
        }}
      />
    </View>
  );
}

const s = StyleSheet.create({
  head: { flexDirection: 'row', gap: 14, alignItems: 'center' },
  title: { fontSize: 19, fontWeight: TITLE_WEIGHT },
  box: { borderWidth: 1.5, borderRadius: 22, padding: 16, gap: 14 },
  cmd: { fontFamily: mono, fontSize: 20, lineHeight: 29, borderRadius: 14, padding: 14, overflow: 'hidden' },
  detail: { fontFamily: mono, fontSize: 13, lineHeight: 19, borderRadius: 14, padding: 12, overflow: 'hidden' },
});
