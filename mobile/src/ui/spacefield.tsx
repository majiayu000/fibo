// The 空间 field of a new task: where its tab will be on the computer, said before it opens.
// Tap to list the spaces; a folder in none can become one from here.
import { useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';
import { tr } from '../i18n';
import { SpaceMark, Icon } from './icons';
import { Label, mono } from './kit';
import { type Landing, type Space, holds, landingNote } from './spaces';
import { tilde } from './status';
import { useTheme } from './theme';

export function SpaceField({
  spaces,
  land,
  cwd,
  canAdd,
  onPick,
  onAdd,
}: {
  spaces: Space[];
  land: Landing;
  cwd?: string;
  /** The computer can make a folder a space (`addspace`). */
  canAdd: boolean;
  onPick: (s: Space) => void;
  onAdd: (path: string) => Promise<void>;
}) {
  const t = useTheme();
  const [open, setOpen] = useState(false);
  const [adding, setAdding] = useState(false);
  const note = landingNote(land, cwd);
  // A folder that is no space's own folder can be made one; its own space then holds it.
  const addable = canAdd && !!cwd && spaces.length > 0 && !spaces.some((s) => s.cwd.replace(/\/+$/, '') === cwd.replace(/\/+$/, ''));
  const s0 = land.space;
  return (
    <View style={s.field}>
      <Label>{tr('空间')}</Label>
      <Pressable
        disabled={spaces.length === 0}
        onPress={() => setOpen(!open)}
        accessibilityRole="button"
        accessibilityLabel={tr('空间：{name}', { name: s0?.name ?? tr('没有') })}
        style={[s.sel, { backgroundColor: t.card, borderColor: t.line }]}
      >
        {s0 ? <SpaceMark name={s0.name} id={s0.id} size={20} radius={5} /> : <Icon name="folder" size={18} color={t.dim} />}
        <View style={{ flex: 1, gap: 2 }}>
          <Text numberOfLines={1} style={{ fontSize: 15, fontWeight: '500', color: s0 ? t.text : t.dim }}>
            {s0?.name || (spaces.length ? tr('选一个空间') : cwd ? tr('新空间（用这个文件夹）') : tr('电脑上还没有空间，选了目录就建一个'))}
          </Text>
          {!!s0?.cwd && (
            <Text numberOfLines={1} ellipsizeMode="head" style={{ fontSize: 12, color: t.dim, fontFamily: mono }}>
              {tilde(s0.cwd)}
            </Text>
          )}
        </View>
        {spaces.length > 0 && (
          <View style={{ transform: [{ rotate: open ? '90deg' : '0deg' }] }}>
            <Icon name="right" size={14} width={2} color={t.dim} />
          </View>
        )}
      </Pressable>
      {!!note && !open && <Text style={{ fontSize: 12, color: t.wait, lineHeight: 17 }}>{note}</Text>}
      {open &&
        spaces.map((x) => {
          const on = x.id === s0?.id;
          return (
            <Pressable
              key={x.id}
              onPress={() => {
                onPick(x);
                setOpen(false);
              }}
              accessibilityRole="button"
              accessibilityState={{ selected: on }}
              style={[s.sel, { backgroundColor: t.bg, borderColor: on ? t.focus : t.line }]}
            >
              <SpaceMark name={x.name} id={x.id} size={20} radius={5} />
              <View style={{ flex: 1, gap: 2 }}>
                <Text numberOfLines={1} style={{ fontSize: 15, color: t.text }}>
                  {x.name}
                </Text>
                <Text numberOfLines={1} ellipsizeMode="head" style={{ fontSize: 12, color: t.dim, fontFamily: mono }}>
                  {x.cwd ? tilde(x.cwd) : tr('没有固定文件夹')}
                  {cwd && x.cwd && holds(x.cwd, cwd) ? `  · ${tr('目录在这里')}` : ''}
                </Text>
              </View>
              {on && <Icon name="check" size={16} width={2.6} color={t.link} />}
            </Pressable>
          );
        })}
      {(open || !!note) && addable && (
        <Pressable
          disabled={adding}
          onPress={() => {
            setAdding(true);
            onAdd(cwd)
              .then(() => setOpen(false))
              .finally(() => setAdding(false));
          }}
          accessibilityRole="button"
          style={[s.sel, s.add, { borderColor: t.line }]}
        >
          {adding ? <ActivityIndicator size="small" /> : <Icon name="plus" size={16} width={2.2} color={t.link} />}
          <Text numberOfLines={1} style={{ flex: 1, fontSize: 15, color: t.link }}>
            {tr('把「{name}」设为新空间', { name: cwd.replace(/\/+$/, '').split('/').pop() || cwd })}
          </Text>
        </Pressable>
      )}
    </View>
  );
}

const s = StyleSheet.create({
  field: { gap: 6 },
  sel: { minHeight: 52, borderRadius: 12, borderWidth: 1, flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 14, paddingVertical: 6 },
  add: { borderStyle: 'dashed', minHeight: 44 },
});
