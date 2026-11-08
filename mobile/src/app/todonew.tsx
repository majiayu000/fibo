// 记一件待办: a title, your words if you want them kept, and the space it belongs to. It goes
// on the computer's list as if written there; no Agent starts until you say 接着做.
import { router } from 'expo-router';
import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { oops } from '@/actions';
import { refresh, supports, useHub, write } from '@/remote/hub';
import { Label, Nav, Wide, noOutline } from '@/ui/kit';
import { KeyboardSafe, useBottomOverKeyboard } from '@/ui/keyboard';
import { useTheme } from '@/ui/theme';

export default function TodoNew() {
  const t = useTheme();
  const bottom = useBottomOverKeyboard();
  const { machines } = useHub();
  const able = machines.filter((m) => supports(m, 'todoadd'));
  const [machine, setMachine] = useState(able[0]?.id ?? '');
  const m = able.find((x) => x.id === machine) ?? able[0];
  const spaces = Object.entries(m?.spaces ?? {}).sort((a, b) => a[1].localeCompare(b[1]));
  const [space, setSpace] = useState('');
  const chosen = spaces.some(([id]) => id === space) ? space : (spaces[0]?.[0] ?? '');
  const [title, setTitle] = useState('');
  const [said, setSaid] = useState('');
  const [busy, setBusy] = useState(false);
  const ready = !!m && !!chosen && !!title.trim() && !busy;

  async function save() {
    if (!m || !ready) return;
    setBusy(true);
    try {
      await write(m.id, 'todoadd', { title: title.trim(), said: said.trim(), space: chosen });
      await refresh(m.id);
      router.back();
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  const chip = (key: string, label: string, on: boolean, onPress: () => void) => (
    <Pressable
      key={key}
      accessibilityRole="button"
      accessibilityState={{ selected: on }}
      onPress={onPress}
      style={[s.chip, { borderColor: on ? t.act : t.line, backgroundColor: on ? t.act : t.card }]}
    >
      <Text style={{ fontSize: 14, color: on ? t.onAct : t.text }}>{label}</Text>
    </Pressable>
  );

  return (
    <KeyboardSafe style={{ backgroundColor: t.bg }}>
      <Nav close big title="记一件待办" sub="记在电脑的待办里，要做时点「接着做」" />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 16, paddingBottom: 16 }} keyboardShouldPersistTaps="handled">
        {!m && <Text style={{ fontSize: 14, color: t.dim }}>没有能记待办的电脑：电脑要在线、更新到新版本，这台手机要有完全控制权限。</Text>}
        <View style={s.field}>
          <Label>要做的事</Label>
          <TextInput
            value={title}
            onChangeText={setTitle}
            placeholder="一句话写清要做什么"
            placeholderTextColor={t.dim}
            autoFocus
            returnKeyType="done"
            onSubmitEditing={save}
            style={[s.input, { color: t.text, borderColor: t.line, backgroundColor: t.card }, noOutline]}
          />
        </View>
        <View style={s.field}>
          <Label>你说的（可不填）</Label>
          <TextInput
            value={said}
            onChangeText={setSaid}
            placeholder="背景、要求，交给 Agent 时原样带上"
            placeholderTextColor={t.dim}
            multiline
            style={[s.input, { minHeight: 96, textAlignVertical: 'top', color: t.text, borderColor: t.line, backgroundColor: t.card }, noOutline]}
          />
        </View>
        {able.length > 1 && (
          <View style={s.field}>
            <Label>电脑</Label>
            <View style={s.chips}>{able.map((x) => chip(x.id, x.name || '电脑', x.id === m?.id, () => setMachine(x.id)))}</View>
          </View>
        )}
        {spaces.length > 0 && (
          <View style={s.field}>
            <Label>空间</Label>
            <View style={s.chips}>{spaces.map(([id, name]) => chip(id, name, id === chosen, () => setSpace(id)))}</View>
          </View>
        )}
      </ScrollView>
      <View style={{ paddingHorizontal: 18, paddingTop: 8, paddingBottom: bottom + 12 }}>
        <Wide label={busy ? '正在记…' : '记下'} disabled={!ready} onPress={save} />
      </View>
    </KeyboardSafe>
  );
}

const s = StyleSheet.create({
  field: { gap: 8 },
  input: { borderWidth: 1, borderRadius: 14, paddingHorizontal: 14, paddingVertical: 12, fontSize: 16 },
  chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  chip: { height: 36, borderRadius: 18, borderWidth: 1, justifyContent: 'center', paddingHorizontal: 14 },
});
