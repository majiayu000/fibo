// Settings kept on this phone: when to be told (on Android, with 后台保持连接 on; push is not
// wired), appearance, Face ID / fingerprint, and a second confirmation for dangerous commands.
import Constants from 'expo-constants';
import * as LocalAuthentication from 'expo-local-authentication';
import { type ReactNode, useEffect, useState } from 'react';
import { AppState, Linking, Platform, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { oops } from '@/actions';
import { notUi, tk, tr } from '@/i18n';
import { keepsConnected } from '@/remote/alerts';
import { askBatteryFree, batteryFree, canKeepConnected, setKeepConnected } from '@/remote/background';
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

const APPEARANCE: Record<Settings['appearance'], string> = { system: tk('跟随系统'), light: tk('浅色'), dark: tk('深色') };
const NEXT: Record<Settings['appearance'], Settings['appearance']> = { system: 'light', light: 'dark', dark: 'system' };
// A language goes by its own name in every language; only 跟随系统 is translated.
const LANGUAGE: Record<Settings['language'], string> = { auto: tk('跟随系统'), zh: notUi('中文'), en: 'English' };
const NEXT_LANGUAGE: Record<Settings['language'], Settings['language']> = { auto: 'zh', zh: 'en', en: 'auto' };

export default function SettingsScreen() {
  const t = useTheme();
  const bottom = useBottom();
  const v = useSettings();
  const stats = useSuggestionStats();
  // A setting changes once it is saved; a failed save says so rather than showing what was not kept.
  const save = (patch: Partial<Settings> | ((value: Settings) => Settings)) => settings.set(patch).catch(() => oops(tr('设置没有保存，请重试')));
  const notify = (k: keyof Settings['notify']) => (on: boolean) => save((value) => ({ ...value, notify: { ...value.notify, [k]: on } }));
  const away = canKeepConnected && keepsConnected(v);
  // The system's answer arrives by coming back to this page, as does a change made in its settings.
  const [free, setFree] = useState(batteryFree);
  useEffect(() => {
    const sub = AppState.addEventListener('change', (s) => s === 'active' && setFree(batteryFree()));
    return () => sub.remove();
  }, []);

  async function setLock(on: boolean) {
    // A browser has no Face ID: a lock it could not check would let everything through.
    if (on && Platform.OS === 'web') return oops(tr('请在手机上设置 Face ID / 指纹锁'));
    try {
      if (on) {
        const has = await LocalAuthentication.hasHardwareAsync().catch(() => false);
        const enrolled = has && (await LocalAuthentication.isEnrolledAsync().catch(() => false));
        if (!enrolled) return oops(tr('这台手机没有设置 Face ID 或指纹'));
        const r = await LocalAuthentication.authenticateAsync({ promptMessage: tr('打开 Face ID / 指纹锁') });
        if (!r.success) return;
      }
      await save({ lock: on });
    } catch {
      oops(tr('无法完成身份验证，请重试'));
    }
  }

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav big title={tr('通知与设置')} />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 18, paddingTop: 8, gap: 12, paddingBottom: bottom + 16 }}>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
          <Label>{tr('什么时候提醒我')}</Label>
          {!away && <Text style={[s.badge, { color: t.wait, backgroundColor: t.waitBg }]}>{canKeepConnected ? tr('关着 App 收不到') : tr('推送还没接')}</Text>}
        </View>
        {canKeepConnected && (
          <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
            <Item t={t} title={tr('后台保持连接')} sub={tr('关着 App 也能收到下面的提醒。通知栏会常驻一条，更费电')}>
              <Switch
                label={tr('后台保持连接')}
                on={away}
                onChange={(on) => {
                  setKeepConnected(on).then(
                    (now) => {
                      if (on && !now) oops(tr('手机没有允许 Fibo 发通知，或系统拦下了后台连接'));
                    },
                    () => oops(tr('设置没有保存，请重试')),
                  );
                }}
              />
            </Item>
            {away && !free && (
              <Item t={t} title={tr('还要允许不限制电量')} sub={tr('不允许的话，手机一离开 Fibo，几秒后连接就会被系统断开，收不到提醒')} onPress={askBatteryFree}>
                <Icon name="right" size={14} width={2} color={t.wait} />
              </Item>
            )}
            <Item t={t} title={tr('系统里的通知和电池设置')} sub={tr('要允许通知、允许后台运行，手机才不会把它杀掉')} last onPress={() => Linking.openSettings()}>
              <Icon name="right" size={14} width={2} color={t.dim} />
            </Item>
          </View>
        )}
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
          <Item t={t} title={tr('等你审批')} sub={tr('立即提醒，点开处理')}>
            <Switch label={tr('等你审批')} on={v.notify.approval} onChange={notify('approval')} />
          </Item>
          <Item t={t} title={tr('等你回答')} sub={tr('Agent 问你问题停下来时')}>
            <Switch label={tr('等你回答')} on={v.notify.waiting} onChange={notify('waiting')} />
          </Item>
          <Item t={t} title={tr('出错了')} sub={tr('命令失败或 Agent 报错')}>
            <Switch label={tr('出错了')} on={v.notify.error} onChange={notify('error')} />
          </Item>
          <Item t={t} title={tr('等你验收')} sub={tr('一轮跑完，等你看结果')} last>
            <Switch label={tr('等你验收')} on={v.notify.done} onChange={notify('done')} />
          </Item>
        </View>
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
          <Item t={t} title={tr('免打扰')} sub={tr('23:00 – 08:00，审批除外')} last>
            <Switch label={tr('免打扰')} on={v.notify.quiet} onChange={notify('quiet')} />
          </Item>
        </View>
        <Text style={{ fontSize: 12, color: t.dim, lineHeight: 18 }}>
          {canKeepConnected
            ? tr('电脑睡着或没联网时没有提醒。提醒由这台手机自己生成，中继看不到内容。')
            : tr('推送需要 APNs / FCM 账号，这一版还没接：开关先记在这台手机上，接好后按这里的设置提醒。')}
        </Text>
        <Label style={{ marginTop: 8 }}>{tr('外观与安全')}</Label>
        <View style={[s.sheet, { backgroundColor: t.card, borderColor: t.line }]}>
          <Item t={t} title={tr('外观')} onPress={() => save({ appearance: NEXT[v.appearance] })}>
            <Text style={{ fontSize: 15, color: t.dim }}>{tr(APPEARANCE[v.appearance])}</Text>
            <Icon name="right" size={14} width={2} color={t.dim} />
          </Item>
          <Item t={t} title={tr('语言')} onPress={() => save({ language: NEXT_LANGUAGE[v.language] })}>
            <Text style={{ fontSize: 15, color: t.dim }}>{v.language === 'auto' ? tr(LANGUAGE.auto) : LANGUAGE[v.language]}</Text>
            <Icon name="right" size={14} width={2} color={t.dim} />
          </Item>
          <Item t={t} title={tr('打开时要 Face ID / 指纹')} sub={tr('别人拿到手机也批不了你的命令')}>
            <Switch label={tr('打开时要 Face ID / 指纹')} on={v.lock} onChange={setLock} />
          </Item>
          <Item t={t} title={tr('允许前再确认一次')} sub={tr('只对 push、rm、部署这类命令')} last>
            <Switch label={tr('允许前再确认一次')} on={v.confirmDanger} onChange={(on) => save({ confirmDanger: on })} />
          </Item>
        </View>
        <Label style={{ marginTop: 8 }}>{tr('输入建议')}</Label>
        <Text style={{ fontSize: 12, color: t.dim, lineHeight: 18 }}>{statsWords(stats)}</Text>
        <Text style={{ textAlign: 'center', fontSize: 13, color: t.dim, marginTop: 24 }}>
          {tr('Fibo {version} · 这些设置只存在这台手机上', { version: Constants.expoConfig?.version ?? '' })}
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
