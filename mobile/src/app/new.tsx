// A new task: pick the computer, the directory and the Agent, write what it should do;
// the computer opens a new tab running that Agent with it as the first message. Or pick
// up the last conversation there: then what you write is optional, its next message.
// What it should do is written in the same box as a conversation's (composer.tsx), typed or held
// to talk, and sending starts it (user, 2026-11-29: 「它和我们的正常的输入方式不太一样，它就是搞成了两套了」).
// The space its tab goes in is shown and sent, so it never lands somewhere unsaid; the
// Pictures picked here go with the first message, on a computer that says it takes them.
import { router } from 'expo-router';
import { useCallback, useEffect, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { confirm, oops } from '@/actions';
import { lang, tr } from '@/i18n';
import { NOT_YET, refresh, remote, speaksV2, supports, useHub, write } from '@/remote/hub';
import { getDraft, resetDraft, setDraft, useDraft } from '@/store/draft';
import { rememberSent } from '@/store/sentPictures';
import { Composer } from '@/ui/composer';
import { Icon } from '@/ui/icons';
import { Label, Nav, Notice, mono } from '@/ui/kit';
import type { Picture } from '@/ui/pictures';
import { KeyboardSafe } from '@/ui/keyboard';
import { SpaceField } from '@/ui/spacefield';
import { addSpaceAsk, ignoredPick, landing, pickSpace } from '@/ui/spaces';
import { tilde } from '@/ui/status';
import { ago, useTheme } from '@/ui/theme';

export default function NewTask() {
  const t = useTheme();
  const { machines } = useHub();
  const draft = useDraft();
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState('');
  const online = machines.filter((m) => m.status === 'online');
  const m = machines.find((x) => x.id === draft.machine) ?? online[0] ?? machines[0];
  const transcribe = m && supports(m, 'transcribe') ? (audio: string) => write(m.id, 'transcribe', { audio, lang: lang() }).then((r) => String(r?.text ?? '')) : undefined;
  const keep = useCallback((d: { text: string }) => {
    if (d.text !== getDraft().prompt) setDraft({ prompt: d.text });
  }, []);

  const selectedMachine = m?.id;
  useEffect(() => {
    if (selectedMachine && draft.machine !== selectedMachine) setDraft({ machine: selectedMachine });
  }, [selectedMachine, draft.machine]);

  const able = supports(m, 'open') && speaksV2(m);
  const spaces = m?.spaceList ?? [];
  const land = landing(spaces, draft.space, draft.cwd);
  // No directory picked: the space's own folder, as a new tab in that space opens there.
  const cwd = draft.cwd || land.space?.cwd || undefined;
  const canResume = able && !!cwd && supports(m, 'lastsession');
  // Older computers start the task without them: no button there rather than lost pictures.
  const canPicture = able && supports(m, 'openimages') && supports(m, 'image');
  /** What stops it starting, said above the box: a send that does nothing says nothing. */
  const missing = busy
    ? ''
    : !m
      ? tr('先在「我的」里配对一台电脑')
      : m.status !== 'online'
        ? tr('电脑没连上')
        : !able
          ? tr('这台电脑的 Fibo 还不能开新会话')
          : !cwd
            ? tr('先选一个目录')
            : '';

  // Another directory, Agent or computer: another 「上次」.
  useEffect(() => {
    setNote('');
    if (getDraft().resume) setDraft({ resume: undefined });
  }, [draft.cwd, draft.agent, draft.machine]);

  /** Finds the last conversation here. One still running is not opened again: you go to it. */
  async function findLast() {
    const r = m ? remote(m.id) : null;
    if (!m || !r || !cwd) return;
    setNote('');
    try {
      const { session: found } = await r.request('lastsession', { cwd: cwd, agent: draft.agent });
      if (!found) setNote(tr('这个目录里没有 {agent} 的旧对话', { agent: m?.agentNames?.[draft.agent] || draft.agent }));
      else if (found.terminal) router.replace({ pathname: '/chat', params: { machine: m.id, terminal: found.terminal } });
      else setDraft({ resume: { id: found.id, title: found.title, updated: found.updated } });
    } catch (e) {
      setNote((e as Error).message);
    }
  }

  /** The box's send, or letting go of a hold: false gives the words back to the box. */
  async function start(prompt: string, pictures: Picture[]): Promise<boolean> {
    if (!m || !cwd || missing) {
      oops(missing || tr('先选一个目录'));
      return false;
    }
    setBusy(true);
    try {
      const space = land.space?.id;
      const images: string[] = [];
      for (const p of canPicture ? pictures : []) images.push((await write(m.id, 'image', { data: p.base64 })).path);
      const where = { ...(space ? { space } : {}), ...(images.length ? { images } : {}) };
      const ok = await write(m.id, 'open', draft.resume ? { resume: draft.resume.id, ...where, ...(prompt ? { prompt } : {}) } : { cwd: cwd, agent: draft.agent, prompt, ...where });
      if (ok?.terminal && images.length) rememberSent(m.id, ok.terminal, prompt, pictures);
      const ignored = ignoredPick(spaces, space, cwd, ok ?? {});
      resetDraft();
      await refresh(m.id);
      if (ignored) oops(ignored);
      if (ok?.terminal) router.replace({ pathname: '/chat', params: { machine: m.id, terminal: ok.terminal } });
      else router.back();
      return true;
    } catch (e) {
      oops((e as Error).message);
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function addSpace(path: string) {
    if (!m) return;
    const ask = addSpaceAsk(path);
    if (!(await confirm(ask.title, ask.detail, ask.ok))) return;
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
      <Nav close big title={tr('新会话')} sub={tr('在电脑上开一个新终端，运行 Agent')} />
      <ScrollView style={{ flex: 1 }} contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 16, paddingBottom: 16 }} keyboardShouldPersistTaps="handled" keyboardDismissMode="interactive">
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
        {!!missing && <Notice text={missing} />}
        {busy && <Notice text={tr('正在打开…')} />}
        {!missing && !busy && (
          <Text style={{ textAlign: 'center', fontSize: 12, color: t.dim }}>
            {land.space ? tr('电脑上「{name}」里会出现一个新标签页，回到电脑前可以直接接着用', { name: land.space.name }) : tr('电脑上会出现一个新标签页，你回到电脑前可以直接接着用')}
          </Text>
        )}
      </ScrollView>
      <Composer
        agent={m?.agentNames?.[draft.agent] || draft.agent}
        // Two Agents: a tap on the name is the other one.
        onModel={() => setDraft({ agent: draft.agent === 'claude' ? 'codex' : 'claude' })}
        running={false}
        canStop={false}
        offline={false}
        onStop={() => undefined}
        onSend={start}
        onError={oops}
        canAttach={canPicture}
        onResume={canResume && !draft.resume ? findLast : undefined}
        sendsEmpty={!!draft.resume}
        placeholder={draft.resume ? tr('接着说什么（可以不写）') : undefined}
        kept={{ text: draft.prompt, pictures: [] }}
        onKeep={keep}
        onTranscribe={transcribe}
      />
    </KeyboardSafe>
  );
}

const s = StyleSheet.create({
  field: { gap: 6 },
  sel: { height: 48, borderRadius: 12, borderWidth: 1, flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 14 },
  selText: { fontSize: 15, fontWeight: '500' },
  dot: { width: 8, height: 8, borderRadius: 4 },
  resume: { flexDirection: 'row', alignItems: 'center', gap: 10, borderWidth: 1, borderRadius: 14, padding: 14 },
  clear: { width: 28, height: 28, borderRadius: 14, alignItems: 'center', justifyContent: 'center' },
});
