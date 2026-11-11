// What the composer's + opens, as Claude's own app does it: what to put into the message on
// top (take a photo, pick pictures), and below it this conversation's settings with what each
// is now. The permission lives here rather than as a pill in the composer: most sessions run
// with every tool allowed, so a pill there said nothing while taking half the row.
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { Icon } from './icons';
import { isFull } from './modes';
import { Frame } from './sheets';
import { useTheme } from './theme';
import { tr } from '../i18n';

export function AttachSheet({
  onClose,
  pictures,
  onCamera,
  onLibrary,
  permission,
  onPermission,
}: {
  onClose: () => void;
  /** Why pictures cannot be added now, or null when they can; undefined: this computer takes none. */
  pictures: string | null | undefined;
  onCamera: () => void;
  onLibrary: () => void;
  /** What the computer says is on; onPermission absent: it cannot be changed from here. */
  permission?: string | null;
  onPermission?: () => void;
}) {
  const t = useTheme();
  const can = pictures === null;
  return (
    <Frame visible onClose={onClose}>
      <View style={s.head}>
        <Text style={[s.title, { color: t.text }]}>{tr('添加到对话')}</Text>
        <Pressable accessibilityRole="button" accessibilityLabel={tr('关闭')} onPress={onClose} hitSlop={8} style={[s.close, { borderColor: t.line }]}>
          <Icon name="close" size={16} width={2} color={t.text} />
        </Pressable>
      </View>
      {pictures !== undefined && (
        <>
          <View style={s.cards}>
            <Card label={tr('拍照')} icon="camera" disabled={!can} onPress={onCamera} />
            <Card label={tr('照片')} icon="image" disabled={!can} onPress={onLibrary} />
          </View>
          {!can && <Text style={{ color: t.dim, fontSize: 13, lineHeight: 18 }}>{pictures}</Text>}
        </>
      )}
      {(!!permission || !!onPermission) && (
        <View style={[s.group, { backgroundColor: t.well }]}>
          <Pressable
            accessibilityRole={onPermission ? 'button' : undefined}
            accessibilityLabel={tr('权限：{role}', { role: permission ?? tr('还没读到') })}
            disabled={!onPermission}
            onPress={onPermission}
            style={({ pressed }) => [s.row, pressed && { opacity: 0.6 }]}
          >
            <View style={[s.rowIcon, { backgroundColor: t.card }]}>
              <Icon name="shield" size={16} width={1.8} color={t.text} />
            </View>
            <Text style={{ flex: 1, color: t.text, fontSize: 15 }}>{tr('权限')}</Text>
            <Text style={{ color: isFull(permission) ? t.err : t.dim, fontSize: 14 }}>{permission ?? tr('还没读到')}</Text>
            {!!onPermission && <Icon name="right" size={14} width={2} color={t.dim} />}
          </Pressable>
        </View>
      )}
    </Frame>
  );
}

function Card({ label, icon, disabled, onPress }: { label: string; icon: 'camera' | 'image'; disabled: boolean; onPress: () => void }) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [s.card, { backgroundColor: t.well, opacity: disabled ? 0.4 : pressed ? 0.7 : 1 }]}
    >
      <View style={[s.cardIcon, { backgroundColor: t.card }]}>
        <Icon name={icon} size={22} width={1.8} color={t.text} />
      </View>
      <Text style={{ color: t.text, fontSize: 14 }}>{label}</Text>
    </Pressable>
  );
}

const s = StyleSheet.create({
  head: { flexDirection: 'row', alignItems: 'center', marginBottom: 12 },
  title: { flex: 1, fontSize: 17, fontWeight: '600' },
  close: { width: 36, height: 36, borderRadius: 18, borderWidth: 1, alignItems: 'center', justifyContent: 'center' },
  cards: { flexDirection: 'row', gap: 10, marginBottom: 12 },
  card: { flex: 1, height: 104, borderRadius: 18, alignItems: 'center', justifyContent: 'center', gap: 8 },
  cardIcon: { width: 44, height: 44, borderRadius: 22, alignItems: 'center', justifyContent: 'center' },
  group: { borderRadius: 18, overflow: 'hidden' },
  row: { minHeight: 56, flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 14 },
  rowIcon: { width: 32, height: 32, borderRadius: 16, alignItems: 'center', justifyContent: 'center' },
});
