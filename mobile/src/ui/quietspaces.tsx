// The spaces put aside on a computer (先不管), at the foot of the home screen: their
// conversations stay off the phone, as the computer decided, but the spaces are named here and
// each can be taken back. 接着管 is the same switch as the computer's.
import { useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';
import { oops, setSpaceQuiet } from '../actions';
import { tr } from '../i18n';
import { type Machine, supports } from '../remote/hub';
import { type Aside, asideSpaces } from './spaces';
import { Group } from './kit';
import { useTheme } from './theme';

export function QuietSpaces({ machines, open, onToggle, named }: { machines: Machine[]; open: boolean; onToggle: () => void; named: boolean }) {
  const t = useTheme();
  const [busy, setBusy] = useState('');
  const aside = asideSpaces(machines);
  if (!aside.length) return null;
  async function back(a: Aside<Machine>) {
    setBusy(`${a.m.id}/${a.id}`);
    try {
      await setSpaceQuiet(a.m.id, a.id, false);
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setBusy('');
    }
  }
  return (
    <>
      <Group label={tr('先不管的空间 · {n}', { n: aside.length })} open={open} onPress={onToggle} />
      {open && (
        <Text style={[s.note, { color: t.dim }]}>{tr('在电脑上点了「先不管」，这些空间的对话不会出现在手机上。')}</Text>
      )}
      {open &&
        aside.map((a) => {
          const key = `${a.m.id}/${a.id}`;
          const can = supports(a.m, 'spacequiet');
          return (
            <View key={key} style={[s.row, { borderColor: t.sep }]}>
              <View style={{ flex: 1, minWidth: 0 }}>
                <Text numberOfLines={1} style={{ color: t.text, fontSize: 15 }}>
                  {a.name}
                </Text>
                {named && (
                  <Text numberOfLines={1} style={{ color: t.dim, fontSize: 12 }}>
                    {a.m.name || tr('电脑')}
                  </Text>
                )}
              </View>
              {can && (
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel={tr('接着管 {name}', { name: a.name })}
                  disabled={!!busy}
                  onPress={() => back(a)}
                  style={({ pressed }) => [s.btn, { borderColor: t.line }, pressed && { backgroundColor: t.well }, !!busy && busy !== key && { opacity: 0.5 }]}
                >
                  {busy === key ? <ActivityIndicator color={t.dim} /> : <Text style={{ color: t.text, fontSize: 14 }}>{tr('接着管')}</Text>}
                </Pressable>
              )}
            </View>
          );
        })}
    </>
  );
}

const s = StyleSheet.create({
  note: { fontSize: 13, lineHeight: 18, paddingHorizontal: 18, paddingBottom: 6 },
  row: { flexDirection: 'row', alignItems: 'center', gap: 12, minHeight: 56, paddingHorizontal: 18, borderBottomWidth: 1 },
  btn: { height: 34, minWidth: 76, paddingHorizontal: 14, borderRadius: 17, borderWidth: 1, alignItems: 'center', justifyContent: 'center' },
});
