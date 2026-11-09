// How hard it thinks, under the models in the model sheet: Claude Code's own /model shows its
// effort there too, and Codex picks it as the second step of /model. One chip per level; the
// computer reads which is on off the CLI after the models, never at the same time (both open
// menus in the same terminal).
import { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';
import { oops, switchMode } from '../actions';
import { remote } from '../remote/hub';
import type { ModeOption, ModeOptions } from './modes';
import { useTheme } from './theme';

export function EffortRow({ machine, terminal, start }: { machine: string; terminal: string; /** The models have been read: the terminal is free. */ start: boolean }) {
  const t = useTheme();
  const [got, setGot] = useState<ModeOptions | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState('');
  useEffect(() => {
    if (!start) return;
    let live = true;
    const r = remote(machine);
    if (!r) return;
    r.request('modes', { terminal, kind: 'effort' })
      .then((x) => live && setGot(x))
      .catch((e) => live && setError((e as Error).message));
    return () => {
      live = false;
    };
  }, [machine, terminal, start]);
  async function pick(o: ModeOption) {
    if (busy || o.current || !got) return;
    setBusy(o.key);
    try {
      if (await switchMode(machine, terminal, 'effort', o)) {
        setGot({ ...got, options: got.options.map((x) => ({ ...x, current: x.key === o.key })) });
      }
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setBusy('');
    }
  }
  const on = got?.options.find((o) => o.current);
  return (
    <View style={[s.wrap, { borderColor: t.sep }]}>
      <View style={s.head}>
        <Text style={{ color: t.text, fontSize: 15, fontWeight: '600' }}>强度</Text>
        {!!on?.detail && (
          <Text numberOfLines={1} style={{ flex: 1, color: t.dim, fontSize: 13 }}>
            {on.detail}
          </Text>
        )}
      </View>
      {!!error && <Text style={{ color: t.err, fontSize: 13, lineHeight: 18 }}>{error}</Text>}
      {!got && !error && <ActivityIndicator accessibilityLabel="正在读强度" color={t.dim} style={{ alignSelf: 'flex-start' }} />}
      {!!got && (
        // Every level in sight: in a scrolling row the last one stood half off the sheet.
        <View style={s.chips}>
          {got.options.map((o) => (
            <Pressable
              key={o.key}
              accessibilityRole="button"
              accessibilityLabel={`强度 ${o.label}${o.current ? '，正在用' : ''}`}
              accessibilityState={{ selected: !!o.current, busy: busy === o.key }}
              disabled={!!o.current || !!busy}
              onPress={() => pick(o)}
              style={({ pressed }) => [
                s.chip,
                { borderColor: o.current ? t.text : t.line, backgroundColor: o.current ? t.text : pressed ? t.well : 'transparent' },
                !!busy && busy !== o.key && { opacity: 0.5 },
              ]}
            >
              {busy === o.key ? (
                <ActivityIndicator color={t.dim} />
              ) : (
                <Text numberOfLines={1} adjustsFontSizeToFit minimumFontScale={0.8} style={{ color: o.current ? t.bg : t.text, fontSize: 14 }}>{o.label}</Text>
              )}
            </Pressable>
          ))}
        </View>
      )}
    </View>
  );
}

const s = StyleSheet.create({
  wrap: { borderTopWidth: 1, paddingTop: 12, marginTop: 4, gap: 10 },
  head: { flexDirection: 'row', alignItems: 'baseline', gap: 10 },
  chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  chip: { flexGrow: 1, flexBasis: 0, minWidth: 52, height: 36, paddingHorizontal: 4, borderRadius: 18, borderWidth: 1, alignItems: 'center', justifyContent: 'center' },
});
