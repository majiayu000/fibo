// A new task: pick the computer, the directory and the Agent, write what it should do;
// the computer opens a new tab running that Agent with it as the first message. Or pick
// up the last conversation there: then what you write is optional, its next message.
import * as Clipboard from 'expo-clipboard';
import { router } from 'expo-router';
import { useEffect, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { oops } from '@/actions';
import { NOT_YET, refresh, remote, speaksV2, supports, useHub, write } from '@/remote/hub';
import { resetDraft, setDraft, useDraft } from '@/store/draft';
import { Icon } from '@/ui/icons';
import { Btn, Label, Nav, Notice, Wide, mono, noOutline, useBottom } from '@/ui/kit';
import { agentName, tilde } from '@/ui/status';
import { ago, useTheme } from '@/ui/theme';

export default function NewTask() {
  const t = useTheme();
  const bottom = useBottom();
  const { machines } = useHub();
  const draft = useDraft();
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState('');
  const online = machines.filter((m) => m.status === 'online');
  const m = machines.find((x) => x.id === draft.machine) ?? online[0] ?? machines[0];

  useEffect(() => {
    if (m && draft.machine !== m.id) setDraft({ machine: m.id });
  }, [m?.id]);

  const able = supports(m, 'open') && speaksV2(m);
  const canResume = able && !!draft.cwd && supports(m, 'lastsession');
  const ready = able && !!draft.cwd && (!!draft.prompt.trim() || !!draft.resume) && !busy;

  // Another directory, Agent or computer: another 「上次」.
  useEffect(() => {
    setNote('');
    if (draft.resume) setDraft({ resume: undefined });
  }, [draft.cwd, draft.agent, draft.machine]);

  /** Finds the last conversation here. One still running is not opened again: you go to it. */
  async function findLast() {
    const r = m ? remote(m.id) : null;
    if (!m || !r || !draft.cwd) return;
    setNote('');
    try {
      const { session: found } = await r.request('lastsession', { cwd: draft.cwd, agent: draft.agent });
      if (!found) setNote(`这个目录里没有 ${agentName(draft.agent)} 的旧对话`);
      else if (found.terminal) router.replace({ pathname: '/chat', params: { machine: m.id, terminal: found.terminal } });
      else setDraft({ resume: { id: found.id, title: found.title, updated: found.updated } });
    } catch (e) {
      setNote((e as Error).message);
    }
  }

  async function start() {
    if (!m || !draft.cwd) return;
    setBusy(true);
    try {
      const prompt = draft.prompt.trim();
      const ok = await write(m.id, 'open', draft.resume ? { resume: draft.resume.id, ...(prompt ? { prompt } : {}) } : { cwd: draft.cwd, agent: draft.agent, prompt });
      resetDraft();
      await refresh(m.id);
      if (ok?.terminal) router.replace({ pathname: '/chat', params: { machine: m.id, terminal: ok.terminal } });
      else router.back();
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav close big title="新任务" sub="在电脑上开一个新终端，运行 Agent" />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 16, paddingBottom: 16 }} keyboardShouldPersistTaps="handled">
        <View style={s.field}>
          <Label>电脑</Label>
          <Pressable onPress={() => setPicking(!picking)} style={[s.sel, { backgroundColor: t.card, borderColor: t.line }]}>
            <View style={[s.dot, { backgroundColor: m?.status === 'online' ? t.ok : t.dim }]} />
            <Text style={[s.selText, { color: t.text }]}>{m?.name || '还没有配对电脑'}</Text>
            <View style={{ marginLeft: 'auto', transform: [{ rotate: picking ? '90deg' : '0deg' }] }}>
              <Icon name="right" size={14} width={2} color={t.dim} />
            </View>
          </Pressable>
          {picking &&
            machines.map((x) => (
              <Pressable
                key={x.id}
                onPress={() => {
                  setDraft({ machine: x.id, cwd: undefined, about: undefined });
                  setPicking(false);
                }}
                style={[s.sel, { backgroundColor: t.bg, borderColor: x.id === m?.id ? t.focus : t.line }]}
              >
                <View style={[s.dot, { backgroundColor: x.status === 'online' ? t.ok : t.dim }]} />
                <Text style={[s.selText, { color: t.text }]}>{x.name || '电脑'}</Text>
                <Text style={{ marginLeft: 'auto', color: t.dim, fontSize: 13 }}>{x.status === 'online' ? '在线' : '不在线'}</Text>
              </Pressable>
            ))}
        </View>
        <View style={s.field}>
          <Label>目录</Label>
          <Pressable
            disabled={!m}
            onPress={() => m && router.push({ pathname: '/dirs', params: { machine: m.id } })}
            style={[s.sel, { height: 56, backgroundColor: t.card, borderColor: t.line }]}
          >
            <Icon name="folder" size={20} color={t.dim} />
            <View style={{ flex: 1, gap: 2 }}>
              <Text numberOfLines={1} style={{ fontFamily: mono, fontSize: 15, color: draft.cwd ? t.text : t.dim }}>
                {draft.cwd ? tilde(draft.cwd) : '选一个文件夹'}
              </Text>
              {!!draft.about && <Text style={{ fontSize: 12, color: t.dim }}>{draft.about}</Text>}
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
          <Label>{draft.resume ? '接着说什么（可以不写）' : '要它做什么'}</Label>
          <TextInput
            multiline
            value={draft.prompt}
            onChangeText={(prompt) => setDraft({ prompt })}
            placeholder="比如：把审批框里的终端内容换成等宽字体"
            placeholderTextColor={t.dim}
            style={[s.area, { color: t.text, backgroundColor: t.card, borderColor: t.line }, noOutline]}
          />
        </View>
        <View style={{ flexDirection: 'row', gap: 8 }}>
          <Btn
            label="从剪贴板粘贴"
            onPress={() =>
              Clipboard.getStringAsync()
                .then((x) => x && setDraft({ prompt: draft.prompt ? `${draft.prompt}\n${x}` : x }))
                .catch(() => undefined)
            }
          />
          {canResume && !draft.resume && <Btn label="接着上次的对话" onPress={findLast} />}
        </View>
        {!!draft.resume && (
          <View style={[s.resume, { backgroundColor: t.card, borderColor: t.line }]}>
            <View style={{ flex: 1, gap: 2 }}>
              <Text style={{ fontSize: 12, color: t.dim }}>接着上次的对话 · {ago(draft.resume.updated)}</Text>
              <Text numberOfLines={2} style={{ fontSize: 15, color: t.text }}>
                {draft.resume.title}
              </Text>
            </View>
            <Pressable onPress={() => setDraft({ resume: undefined })} style={[s.clear, { backgroundColor: t.well }]} accessibilityLabel="不接着这段对话">
              <Icon name="close" size={12} width={2.4} color={t.text} />
            </Pressable>
          </View>
        )}
        {!!note && <Notice text={note} />}
        {m && m.status === 'online' && !able && <Notice text={NOT_YET} />}
        {m && m.status !== 'online' && <Notice text={m.error ?? '电脑没连上'} />}
      </ScrollView>
      <View style={{ paddingHorizontal: 18, paddingBottom: bottom + 4, gap: 10 }}>
        <Wide label={busy ? '正在打开…' : draft.resume ? '接着做' : '开始'} disabled={!ready} onPress={start} />
        <Text style={{ textAlign: 'center', fontSize: 12, color: t.dim }}>电脑上会出现一个新标签页，你回到电脑前可以直接接着用</Text>
      </View>
    </View>
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
