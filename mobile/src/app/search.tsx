// Finding a conversation: words in its title, space or directory, and the spaces to narrow to,
// the same filter the home screen has. 稍后提醒 and 已处理 are found too.
import { router } from 'expo-router';
import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { type Card, type Machine, useHub } from '@/remote/hub';
import { Icon } from '@/ui/icons';
import { arrange, itemKey, namesComputer, searchCards, shelve } from '@/ui/inbox';
import { back, noOutline, useBottom, useTop } from '@/ui/kit';
import { Row } from '@/ui/row';
import { SpaceChips } from '@/ui/spacechips';
import { useTheme } from '@/ui/theme';

const open = (m: Machine, c: Card) => router.push({ pathname: '/chat', params: { machine: m.id, terminal: c.id } });

export default function Search() {
  const t = useTheme();
  const top = useTop();
  const bottom = useBottom();
  const { machines } = useHub();
  const [query, setQuery] = useState('');
  const [space, setSpace] = useState<string | null>(null);
  const a = arrange(machines, []);
  const { chips, filter } = shelve(a, space);
  const found = searchCards(a, query, filter);
  const asked = query.trim().length > 0 || filter !== null;
  const named = namesComputer(machines);
  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <View style={[s.head, { paddingTop: top + 9 }]}>
        <View style={[s.field, { borderColor: t.line, backgroundColor: t.card }]}>
          <Icon name="search" size={18} width={2} color={t.dim} />
          <TextInput
            accessibilityLabel="搜索会话"
            autoFocus
            value={query}
            onChangeText={setQuery}
            placeholder="搜标题、空间或目录"
            placeholderTextColor={t.dim}
            returnKeyType="search"
            style={[s.input, { color: t.text }, noOutline]}
          />
          {query.length > 0 && (
            <Pressable accessibilityRole="button" accessibilityLabel="清空" hitSlop={10} onPress={() => setQuery('')}>
              <Icon name="close" size={16} width={2} color={t.dim} />
            </Pressable>
          )}
        </View>
        <Pressable accessibilityRole="button" hitSlop={8} onPress={back} style={s.cancel}>
          <Text style={{ fontSize: 15, color: t.text }}>取消</Text>
        </Pressable>
      </View>
      <SpaceChips chips={chips} filter={filter} onPick={setSpace} />
      <ScrollView keyboardShouldPersistTaps="handled" keyboardDismissMode="on-drag" contentContainerStyle={{ paddingBottom: bottom + 16 }}>
        {asked && <Text style={[s.count, { color: t.dim }]}>{found.length > 0 ? `找到 ${found.length} 个` : '没有找到'}</Text>}
        {asked && found.map((i, n) => <Row key={itemKey(i)} m={i.m} c={i.c} named={named} last={n === found.length - 1} onPress={() => open(i.m, i.c)} />)}
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  head: { flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 18, paddingBottom: 10 },
  field: { flex: 1, minWidth: 0, height: 44, borderWidth: 1, borderRadius: 22, flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 14 },
  input: { flex: 1, minWidth: 0, fontSize: 15, padding: 0 },
  cancel: { minHeight: 44, justifyContent: 'center' },
  count: { fontSize: 13, paddingHorizontal: 18, paddingVertical: 6 },
});
