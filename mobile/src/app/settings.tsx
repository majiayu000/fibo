// Settings kept on this phone: when to be told (switches only, push is not wired yet),
// appearance, Face ID / fingerprint, and a second confirmation for dangerous commands.
import Constants from 'expo-constants';
import * as LocalAuthentication from 'expo-local-authentication';
import type { ReactNode } from 'react';
import { Platform, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { oops } from '@/actions';
import { type Settings, settings, useSettings } from '@/store/settings';
import { statsWords } from '@/store/suggestionCounts';
import { useSuggestionStats } from '@/store/suggestions';
import { Icon } from '@/ui/icons';
import { Label, Nav, Switch, TITLE_WEIGHT, useBottom } from '@/ui/kit';
import { type Theme, useTheme } from '@/ui/theme';

function Item({ t, title, sub, children, last, onPress }: { t: Theme; title: string; sub?: string; children?: ReactNode; last?: boolean; onPress?: () => void }) {
  const body = (
    <>
      <View style={{ flex: 1, gap: 3 }}>
        <Text style={{ fontSize: 16, color: t.text, fontWeight: TITLE_WEIGHT }}>{title}</Text>
        {!!sub && <Text style={{ fontSize: 13, color: t.dim }}>{sub}</Text>}
      </View>
      {children}
    </>
  );
  const style = [s.item, { borderColor: t.sep }, last && { borderBottomWidth: 0 }];
  return onPress ? (
    <Pressable onPress={onPress} style={style}>
      {body}
    </Pressable>
  ) : (
    <View style={style}>{body}</View>
  );
}

const APPEARANCE: Record<Settings['appearance'], string> = { system: '跟随系统', light: '浅色', dark: '深色' };
const NEXT: Record<Settings['appearance'], Settings['appearance']> = { system: 'light', light: 'dark', dark: 'system' };

export default function SettingsScreen() {
  const t = useTheme();
  const bottom = useBottom();
  const v = useSettings();
  const stats = useSuggestionStats();
  const notify = (k: keyof Settings['notify']) => (on: boolean) => settings.set({ notify: { ...v.notify, [k]: on } });

  async function setLock(on: boolean) {
    if (on && Platform.OS !== 'web') {
      const has = await LocalAuthentication.hasHardwareAsync().catch(() => false);
      const enrolled = has && (await LocalAuthentication.isEnrolledAsync().catch(() => false));
      if (!enrolled) return oops('这台手机没有设置 Face ID 或指纹');
      const r = await LocalAuthentication.authenticateAsync({ promptMessage: '打开 Face ID / 指纹锁' });
      if (!r.success) return;
    }
    settings.set({ lock: on });
  }

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav big title="通知与设置" />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 12, paddingBottom: bottom + 16 }}>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
          <Label>什么时候提醒我</Label>
          <Text style={[s.badge, { color: t.wait, backgroundColor: t.waitBg }]}>推送还没接</Text>
        </View>
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
          <Item t={t} title="等你审批" sub="立即，带「允许一次」按钮">
            <Switch label="等你审批" on={v.notify.approval} onChange={notify('approval')} />
          </Item>
          <Item t={t} title="等你回答" sub="Agent 问你问题停下来时">
            <Switch label="等你回答" on={v.notify.waiting} onChange={notify('waiting')} />
          </Item>
          <Item t={t} title="出错了" sub="命令失败或 Agent 报错">
            <Switch label="出错了" on={v.notify.error} onChange={notify('error')} />
          </Item>
          <Item t={t} title="等你验收" sub="一轮跑完，等你看结果" last>
            <Switch label="等你验收" on={v.notify.done} onChange={notify('done')} />
          </Item>
        </View>
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
          <Item t={t} title="免打扰" sub="23:00 – 08:00，审批除外" last>
            <Switch label="免打扰" on={v.notify.quiet} onChange={notify('quiet')} />
          </Item>
        </View>
        <Text style={{ fontSize: 12, color: t.dim, lineHeight: 18 }}>
          推送需要 APNs / FCM 账号，这一版还没接：开关先记在这台手机上，接好后按这里的设置提醒。
        </Text>
        <Label style={{ marginTop: 8 }}>外观与安全</Label>
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
          <Item t={t} title="外观" onPress={() => settings.set({ appearance: NEXT[v.appearance] })}>
            <Text style={{ fontSize: 15, color: t.dim }}>{APPEARANCE[v.appearance]}</Text>
            <Icon name="right" size={14} width={2} color={t.dim} />
          </Item>
          <Item t={t} title="打开时要 Face ID / 指纹" sub="别人拿到手机也批不了你的命令">
            <Switch label="打开时要 Face ID / 指纹" on={v.lock} onChange={setLock} />
          </Item>
          <Item t={t} title="允许前再确认一次" sub="只对 push、rm、部署这类命令" last>
            <Switch label="允许前再确认一次" on={v.confirmDanger} onChange={(on) => settings.set({ confirmDanger: on })} />
          </Item>
        </View>
        <Label style={{ marginTop: 8 }}>输入建议</Label>
        <Text style={{ fontSize: 12, color: t.dim, lineHeight: 18 }}>{statsWords(stats)}</Text>
        <Text style={{ textAlign: 'center', fontSize: 13, color: t.dim, marginTop: 24 }}>
          Fibo {Constants.expoConfig?.version ?? ''} · 这些设置只存在这台手机上
        </Text>
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  sheet: { borderWidth: 1, borderRadius: 16, overflow: 'hidden' },
  item: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 14, paddingVertical: 12, minHeight: 64, borderBottomWidth: 1 },
  badge: { fontSize: 11, paddingHorizontal: 8, paddingVertical: 2, borderRadius: 8, overflow: 'hidden' },
});
