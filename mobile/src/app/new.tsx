// A new task: pick the computer, the directory and the Agent, write what it should do;
// the computer opens a new tab running that Agent with it as the first message.
import * as Clipboard from 'expo-clipboard';
import { router } from 'expo-router';
import { useEffect, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { oops } from '@/actions';
import { NOT_YET, refresh, speaksV2, supports, useHub, write } from '@/remote/hub';
import { resetDraft, setDraft, useDraft } from '@/store/draft';
import { Icon } from '@/ui/icons';
import { Btn, Label, Nav, Notice, Wide, mono, noOutline, useBottom } from '@/ui/kit';
import { tilde } from '@/ui/status';
import { useTheme } from '@/ui/theme';

export default function NewTask() {
  const t = useTheme();
  const bottom = useBottom();
  const { machines } = useHub();
  const draft = useDraft();
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const online = machines.filter((m) => m.status === 'online');
  const m = machines.find((x) => x.id === draft.machine) ?? online[0] ?? machines[0];

  useEffect(() => {
    if (m && draft.machine !== m.id) setDraft({ machine: m.id });
  }, [m?.id]);

  const able = supports(m, 'open') && speaksV2(m);
  const ready = able && !!draft.cwd && !!draft.prompt.trim() && !busy;

  async function start() {
    if (!m || !draft.cwd) return;
    setBusy(true);
    try {
      const ok = await write(m.id, 'open', { cwd: draft.cwd, agent: draft.agent, prompt: draft.prompt.trim() });
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
          <Label>要它做什么</Label>
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
        </View>
        {m && m.status === 'online' && !able && <Notice text={NOT_YET} />}
        {m && m.status !== 'online' && <Notice text={m.error ?? '电脑没连上'} />}
      </ScrollView>
      <View style={{ paddingHorizontal: 18, paddingBottom: bottom + 4, gap: 10 }}>
        <Wide label={busy ? '正在打开…' : '开始'} disabled={!ready} onPress={start} />
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
  area: { minHeight: 150, borderWidth: 1, borderRadius: 14, padding: 14, fontSize: 15, lineHeight: 22, textAlignVertical: 'top' },
});
