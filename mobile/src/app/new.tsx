// A new task: pick the computer, the directory and the Agent, write what it should do;
// the computer opens a new tab running that Agent with it as the first message. Or pick
// up the last conversation there: then what you write is optional, its next message.
// The space its tab goes in is shown and sent, so it never lands somewhere unsaid; the
// keyboard's return key starts it (or puts the keyboard away and says what is missing).
import * as Clipboard from 'expo-clipboard';
import { router } from 'expo-router';
import { type ComponentRef, useEffect, useRef, useState } from 'react';
import { Keyboard, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { oops } from '@/actions';
import { tr } from '@/i18n';
import { NOT_YET, refresh, remote, speaksV2, supports, useHub, write } from '@/remote/hub';
import { resetDraft, setDraft, useDraft } from '@/store/draft';
import { Icon } from '@/ui/icons';
import { Btn, Label, Nav, Notice, Wide, mono, noOutline } from '@/ui/kit';
import { KeyboardSafe, useBottomOverKeyboard } from '@/ui/keyboard';
import { SpaceField } from '@/ui/spacefield';
import { ignoredPick, landing, pickSpace } from '@/ui/spaces';
import { agentName, tilde } from '@/ui/status';
import { ago, useTheme } from '@/ui/theme';

export default function NewTask() {
  const t = useTheme();
  const bottom = useBottomOverKeyboard();
  const { machines } = useHub();
  const draft = useDraft();
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState('');
  const scroll = useRef<ComponentRef<typeof ScrollView>>(null);
  const typing = useRef(false);
  // The box is the last field: once the keyboard is up and the page has shrunk, bring it
  // into view. On focus alone is too early on Android, where the keyboard is still coming.
  useEffect(() => {
    const sub = Keyboard.addListener('keyboardDidShow', () => typing.current && scroll.current?.scrollToEnd({ animated: true }));
    return () => sub.remove();
  }, []);
  const online = machines.filter((m) => m.status === 'online');
  const m = machines.find((x) => x.id === draft.machine) ?? online[0] ?? machines[0];

  useEffect(() => {
    if (m && draft.machine !== m.id) setDraft({ machine: m.id });
  }, [m?.id]);

  const able = supports(m, 'open') && speaksV2(m);
  const spaces = m?.spaceList ?? [];
  const land = landing(spaces, draft.space, draft.cwd);
  // No directory picked: the space's own folder, as a new tab in that space opens there.
  const cwd = draft.cwd || land.space?.cwd || undefined;
  const canResume = able && !!cwd && supports(m, 'lastsession');
  const ready = able && !!cwd && (!!draft.prompt.trim() || !!draft.resume) && !busy;
  /** What stops 开始, said where the button is: a grey button alone says nothing. */
  const missing = busy
    ? ''
    : !m
      ? tr('先在「我的」里配对一台电脑')
      : m.status !== 'online'
        ? tr('电脑没连上')
        : !able
          ? tr('这台电脑的 Fibo 还不能开新任务')
          : !cwd
            ? tr('先选一个目录')
            : !draft.prompt.trim() && !draft.resume
              ? tr('写上要它做什么')
              : '';

  // Another directory, Agent or computer: another 「上次」.
  useEffect(() => {
    setNote('');
    if (draft.resume) setDraft({ resume: undefined });
  }, [draft.cwd, draft.agent, draft.machine]);

  /** Finds the last conversation here. One still running is not opened again: you go to it. */
  async function findLast() {
    const r = m ? remote(m.id) : null;
    if (!m || !r || !cwd) return;
    setNote('');
    try {
      const { session: found } = await r.request('lastsession', { cwd: cwd, agent: draft.agent });
      if (!found) setNote(tr('这个目录里没有 {agent} 的旧对话', { agent: agentName(draft.agent) }));
      else if (found.terminal) router.replace({ pathname: '/chat', params: { machine: m.id, terminal: found.terminal } });
      else setDraft({ resume: { id: found.id, title: found.title, updated: found.updated } });
    } catch (e) {
      setNote((e as Error).message);
    }
  }

  async function start() {
    if (!m || !cwd) return;
    setBusy(true);
    try {
      const prompt = draft.prompt.trim();
      const space = land.space?.id;
      const where = space ? { space } : {};
      const ok = await write(m.id, 'open', draft.resume ? { resume: draft.resume.id, ...where, ...(prompt ? { prompt } : {}) } : { cwd: cwd, agent: draft.agent, prompt, ...where });
      const ignored = ignoredPick(spaces, space, cwd, ok ?? {});
      resetDraft();
      await refresh(m.id);
      if (ignored) oops(ignored);
      if (ok?.terminal) router.replace({ pathname: '/chat', params: { machine: m.id, terminal: ok.terminal } });
      else router.back();
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  /** The keyboard's return key: start when it can; the keyboard goes away either way, and the line under 开始 says what is missing. */
  function submit() {
    if (ready) start();
  }

  async function addSpace(path: string) {
    if (!m) return;
    try {
      const made = await write(m.id, 'addspace', { path });
      await refresh(m.id);
      if (made?.id) setDraft({ space: made.id });
    } catch (e) {
      oops((e as Error).message);
    }
  }

  return (
    <KeyboardSafe style={{ backgroundColor: t.bg }}>
      <Nav close big title={tr('新任务')} sub={tr('在电脑上开一个新终端，运行 Agent')} />
      <ScrollView ref={scroll} contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 16, paddingBottom: 16 }} keyboardShouldPersistTaps="handled" keyboardDismissMode="interactive">
        <View style={s.field}>
          <Label>{tr('电脑')}</Label>
          <Pressable onPress={() => setPicking(!picking)} style={[s.sel, { backgroundColor: t.card, borderColor: t.line }]}>
            <View style={[s.dot, { backgroundColor: m?.status === 'online' ? t.ok : t.dim }]} />
            <Text style={[s.selText, { color: t.text }]}>{m?.name || tr('还没有配对电脑')}</Text>
            <View style={{ marginLeft: 'auto', transform: [{ rotate: picking ? '90deg' : '0deg' }] }}>
              <Icon name="right" size={14} width={2} color={t.dim} />
            </View>
          </Pressable>
          {picking &&
            machines.map((x) => (
              <Pressable
                key={x.id}
                onPress={() => {
                  setDraft({ machine: x.id, cwd: undefined, about: undefined, space: undefined });
                  setPicking(false);
                }}
                style={[s.sel, { backgroundColor: t.bg, borderColor: x.id === m?.id ? t.focus : t.line }]}
              >
                <View style={[s.dot, { backgroundColor: x.status === 'online' ? t.ok : t.dim }]} />
                <Text style={[s.selText, { color: t.text }]}>{x.name || tr('电脑')}</Text>
                <Text style={{ marginLeft: 'auto', color: t.dim, fontSize: 13 }}>{x.status === 'online' ? tr('在线') : tr('不在线')}</Text>
              </Pressable>
            ))}
        </View>
        {m && (
          <SpaceField
            spaces={spaces}
            land={land}
            cwd={draft.cwd}
            canAdd={supports(m, 'addspace')}
            onAdd={addSpace}
            onPick={(x) => {
              const p = pickSpace(x, draft.cwd);
              setDraft({ space: p.space, cwd: p.cwd, ...(p.reset && { about: undefined }) });
            }}
          />
        )}
        <View style={s.field}>
          <Label>{tr('目录')}</Label>
          <Pressable
            disabled={!m}
            onPress={() => m && router.push({ pathname: '/dirs', params: { machine: m.id, ...(cwd && { at: cwd }) } })}
            style={[s.sel, { height: 56, backgroundColor: t.card, borderColor: t.line }]}
          >
            <Icon name="folder" size={20} color={t.dim} />
            <View style={{ flex: 1, gap: 2 }}>
              <Text numberOfLines={1} ellipsizeMode="head" style={{ fontFamily: mono, fontSize: 15, color: cwd ? t.text : t.dim }}>
                {cwd ? tilde(cwd) : tr('选一个文件夹')}
              </Text>
              {!!draft.about && <Text style={{ fontSize: 12, color: t.dim }}>{draft.about}</Text>}
              {!draft.cwd && !!cwd && <Text style={{ fontSize: 12, color: t.dim }}>{tr('空间「{name}」的文件夹，点这里换', { name: land.space?.name })}</Text>}
            </View>
            <Icon name="right" size={14} width={2} color={t.dim} />
          </Pressable>
        </View>
        <View style={s.field}>
          <Label>Agent</Label>
          <View style={[s.seg, { backgroundColor: t.card, borderColor: t.line }]}>
            {(['claude', 'codex'] as const).map((a) => (
              <Pressable
                key={a}
                onPress={() => setDraft({ agent: a })}
                style={[s.segItem, draft.agent === a && { backgroundColor: t.well }]}
                accessibilityRole="button"
                accessibilityState={{ selected: draft.agent === a }}
              >
                <Text style={{ fontSize: 15, color: draft.agent === a ? t.text : t.dim, fontWeight: draft.agent === a ? '600' : '400' }}>
                  {a === 'claude' ? 'Claude Code' : 'Codex'}
                </Text>
              </Pressable>
            ))}
          </View>
        </View>
        <View style={s.field}>
          <Label>{draft.resume ? tr('接着说什么（可以不写）') : tr('要它做什么')}</Label>
          <TextInput
            multiline
            value={draft.prompt}
            onChangeText={(prompt) => setDraft({ prompt })}
            // Return starts it: on a phone a new line is rarely what is meant, and the
            // keyboard otherwise hides 开始 with no way to put it away.
            returnKeyType={ready ? 'send' : 'done'}
            submitBehavior="blurAndSubmit"
            onSubmitEditing={submit}
            onFocus={() => {
              typing.current = true;
              if (Keyboard.isVisible()) scroll.current?.scrollToEnd({ animated: true });
            }}
            onBlur={() => (typing.current = false)}
            accessibilityLabel={draft.resume ? tr('接着说什么') : tr('要它做什么')}
            placeholder={tr('比如：把审批框里的终端内容换成等宽字体')}
            placeholderTextColor={t.dim}
            style={[s.area, { color: t.text, backgroundColor: t.card, borderColor: t.line }, noOutline]}
          />
        </View>
        <View style={{ flexDirection: 'row', gap: 8 }}>
          <Btn
            label={tr('从剪贴板粘贴')}
            onPress={() =>
              Clipboard.getStringAsync()
                .then((x) => x && setDraft({ prompt: draft.prompt ? `${draft.prompt}\n${x}` : x }))
                .catch(() => undefined)
            }
          />
          {canResume && !draft.resume && <Btn label={tr('接着上次的对话')} onPress={findLast} />}
        </View>
        {!!draft.resume && (
          <View style={[s.resume, { backgroundColor: t.card, borderColor: t.line }]}>
            <View style={{ flex: 1, gap: 2 }}>
              <Text style={{ fontSize: 12, color: t.dim }}>{tr('接着上次的对话 · {when}', { when: ago(draft.resume.updated) })}</Text>
              <Text numberOfLines={2} style={{ fontSize: 15, color: t.text }}>
                {draft.resume.title}
              </Text>
            </View>
            <Pressable onPress={() => setDraft({ resume: undefined })} style={[s.clear, { backgroundColor: t.well }]} accessibilityLabel={tr('不接着这段对话')}>
              <Icon name="close" size={12} width={2.4} color={t.text} />
            </Pressable>
          </View>
        )}
        {!!note && <Notice text={note} />}
        {m && m.status === 'online' && !able && <Notice text={tr(NOT_YET)} />}
        {m && m.status !== 'online' && <Notice text={m.error ?? tr('电脑没连上')} />}
      </ScrollView>
      <View style={{ paddingHorizontal: 18, paddingBottom: bottom + 4, gap: 10 }}>
        <Wide label={busy ? tr('正在打开…') : draft.resume ? tr('接着做') : tr('开始')} disabled={!ready} onPress={start} />
        <Text style={{ textAlign: 'center', fontSize: 12, color: t.dim }}>
          {missing || (land.space ? tr('电脑上「{name}」里会出现一个新标签页，回到电脑前可以直接接着用', { name: land.space.name }) : tr('电脑上会出现一个新标签页，你回到电脑前可以直接接着用'))}
        </Text>
      </View>
    </KeyboardSafe>
  );
}

const s = StyleSheet.create({
  field: { gap: 6 },
  sel: { height: 48, borderRadius: 12, borderWidth: 1, flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 14 },
  selText: { fontSize: 15, fontWeight: '500' },
  dot: { width: 8, height: 8, borderRadius: 4 },
  seg: { flexDirection: 'row', borderWidth: 1, borderRadius: 12, padding: 3, gap: 3 },
  segItem: { flex: 1, height: 38, borderRadius: 9, alignItems: 'center', justifyContent: 'center' },
  resume: { flexDirection: 'row', alignItems: 'center', gap: 10, borderWidth: 1, borderRadius: 14, padding: 14 },
  clear: { width: 28, height: 28, borderRadius: 14, alignItems: 'center', justifyContent: 'center' },
  area: { minHeight: 150, borderWidth: 1, borderRadius: 14, padding: 14, fontSize: 15, lineHeight: 22, textAlignVertical: 'top' },
});
