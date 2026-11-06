// What a composer's tag opens: what the computer read off the CLI, one row each. It says when it
// cannot: the Agent is busy, asking something, or a CLI it does not know.
import { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { oops, switchMode } from '../actions';
import { remote } from '../remote/hub';
import { Frame } from './sheets';
import { kindName, type ModeKind, type ModeOption, type ModeOptions } from './modes';
import { Icon } from './icons';
import { useTheme } from './theme';

export function ModeSheet({ machine, terminal, kind, onClose }: { machine: string; terminal: string; kind: ModeKind; onClose: () => void }) {
  const t = useTheme();
  const [got, setGot] = useState<ModeOptions | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState('');
  useEffect(() => {
    let live = true;
    const r = remote(machine);
    if (!r) {
      setError('电脑没连上');
      return;
    }
    r.request('modes', { terminal, kind })
      .then((x) => live && setGot(x))
      .catch((e) => live && setError((e as Error).message));
    return () => {
      live = false;
    };
  }, [machine, terminal, kind]);
  async function pick(o: ModeOption) {
    if (busy || o.current) return;
    setBusy(o.key);
    try {
      if (await switchMode(machine, terminal, kind, o)) onClose();
    } catch (e) {
      // What it really is goes with the refusal; the tag keeps saying what the computer says.
      oops((e as Error).message);
    } finally {
      setBusy('');
    }
  }
  return (
    <Frame visible onClose={onClose}>
      <Text style={[s.title, { color: t.text }]}>换{kindName(kind)}</Text>
      {!!error && <Text style={{ color: t.err, fontSize: 14, lineHeight: 20 }}>{error}</Text>}
      {!got && !error && <ActivityIndicator accessibilityLabel="正在读" color={t.dim} />}
      <ScrollView style={{ maxHeight: 420 }} keyboardShouldPersistTaps="handled">
        {got?.options.map((o) => (
          <Pressable
            key={o.key}
            accessibilityRole="button"
            accessibilityState={{ selected: !!o.current, busy: busy === o.key, disabled: !!o.current }}
            accessibilityLabel={`${o.label}${o.current ? '，正在用' : ''}`}
            disabled={!!o.current || !!busy}
            onPress={() => pick(o)}
            style={({ pressed }) => [s.row, { borderColor: t.sep }, pressed && { backgroundColor: t.well }, !!busy && busy !== o.key && { opacity: 0.5 }]}
          >
            <View style={{ flex: 1, gap: 2 }}>
              <Text style={{ color: o.full ? t.err : t.text, fontSize: 15 }}>{o.label}</Text>
              {!!o.detail && (
                <Text numberOfLines={2} style={{ color: t.dim, fontSize: 13, lineHeight: 18 }}>
                  {o.detail}
                </Text>
              )}
            </View>
            {busy === o.key ? <ActivityIndicator color={t.dim} /> : o.current ? <Icon name="check" size={16} width={2.2} color={t.text} /> : null}
          </Pressable>
        ))}
      </ScrollView>
      {!!got?.note && <Text style={{ color: t.dim, fontSize: 13, lineHeight: 18 }}>{got.note}</Text>}
    </Frame>
  );
}

const s = StyleSheet.create({
  title: { fontSize: 17, fontWeight: '600', marginBottom: 8 },
  row: { flexDirection: 'row', alignItems: 'center', gap: 10, paddingVertical: 12, borderBottomWidth: 1 },
});
