// Picking the directory a new task runs in: browse the computer's folders level by
// level, search them by name, type a full path, or make a new folder. Folders only; the
// computer gives this to full-control phones alone.
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { oops } from '@/actions';
import { NOT_YET, refresh, remote, supports, useHub, write } from '@/remote/hub';
import { setDraft, useDraft } from '@/store/draft';
import { Icon } from '@/ui/icons';
import { Label, Nav, Notice, Wide, mono, noOutline, useBottom } from '@/ui/kit';
import { trail } from '@/ui/parse';
import { SayDialog } from '@/ui/sheets';
import { pickDir } from '@/ui/spaces';
import { tilde } from '@/ui/status';
import { type Theme, useTheme } from '@/ui/theme';

type Dir = { name: string; path: string; git?: { branch: string; changes?: number | null; worktree: boolean } | null; sessions?: number; recent?: boolean };
type Listing = { path: string; parent?: string | null; home: string; dirs: Dir[]; recent?: string[]; truncated?: boolean };

/** A worktree is said to be one; otherwise its count, when counting did not time out. */
function changes(g: NonNullable<Dir['git']>): string {
  if (g.worktree) return '工作树';
  if (g.changes === undefined || g.changes === null) return '';
  return g.changes ? `${g.changes} 个改动` : '没有改动';
}

function about(d: Dir): string {
  return d.git ? [d.git.branch || 'git', changes(d.git)].filter(Boolean).join(' · ') : '普通文件夹';
}

/** Why a search may have missed some: it ran out of time, or hit the machine's 50. */
function searchLimit(l: Listing): string {
  if (l.truncated) return '没搜完，多打几个字再试';
  return l.dirs.length >= 50 ? '结果太多，只显示前 50 个' : '';
}

function Mark({ text, query, t }: { text: string; query: string; t: Theme }) {
  const at = query ? text.toLowerCase().lastIndexOf(query.toLowerCase()) : -1;
  if (at < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, at)}
      <Text style={{ color: t.link }}>{text.slice(at, at + query.length)}</Text>
      {text.slice(at + query.length)}
    </>
  );
}

export default function Dirs() {
  const t = useTheme();
  const bottom = useBottom();
  // `at`: the directory already chosen, so the picker opens there rather than at ~ again.
  const { machine, at } = useLocalSearchParams<{ machine: string; at?: string }>();
  const { machines } = useHub();
  const draft = useDraft();
  const m = machines.find((x) => x.id === machine);
  const online = m?.status === 'online';
  const can = supports(m, 'dirs');
  const [path, setPath] = useState<string | undefined>(at || undefined);
  const [list, setList] = useState<Listing | null>(null);
  const [query, setQuery] = useState('');
  const [found, setFound] = useState<Dir[] | null>(null);
  const [picked, setPicked] = useState<Dir | null>(null);
  const [error, setError] = useState('');
  const [cut, setCut] = useState('');
  const [making, setMaking] = useState(false);
  const [adding, setAdding] = useState(false);
  // Counting changes in a big tree can take the machine about three seconds.
  const [busy, setBusy] = useState(false);
  const typedPath = /^[~/]/.test(query.trim());

  useEffect(() => {
    const r = online && can ? remote(machine) : null;
    if (!r) return;
    let live = true;
    setBusy(true);
    r.request('dirs', path ? { path } : {})
      .then((l: Listing) => {
        if (!live) return;
        setList(l);
        setError('');
      })
      .catch((e) => {
        if (!live) return;
        // The directory it opened at is gone: home, not a screen with nowhere to go.
        if (path && path === at && !list) setPath(undefined);
        else setError((e as Error).message);
      })
      .finally(() => live && setBusy(false));
    return () => {
      live = false;
    };
  }, [online, can, machine, path]);

  // Searching by name, a moment after typing stops.
  useEffect(() => {
    const q = query.trim();
    if (!q || typedPath) return setFound(null);
    const r = online && can ? remote(machine) : null;
    if (!r) return;
    let live = true;
    setFound(null);
    const timer = setTimeout(() => {
      r.request('dirs', { query: q })
        .then((l: Listing) => {
          if (!live) return;
          setFound(l.dirs);
          setCut(searchLimit(l));
        })
        .catch((e) => live && setError((e as Error).message));
    }, 250);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [query, typedPath, online, can, machine]);

  function choose(cwd: string, d?: Dir | null) {
    const space = m?.spaceDirs?.[cwd];
    const known = [d && about(d), space && `空间 ${space}`].filter(Boolean).join(' · ');
    setDraft({ machine, cwd, about: known || undefined, space: pickDir(m?.spaceList ?? [], draft.space, cwd) });
    router.back();
  }

  async function makeSpace(path: string) {
    setAdding(true);
    try {
      await write(machine, 'addspace', { path });
      await refresh(machine);
    } catch (e) {
      oops((e as Error).message);
    } finally {
      setAdding(false);
    }
  }

  async function mkdir(name: string) {
    const base = list?.path ?? '';
    const target = name.startsWith('/') || name.startsWith('~') ? name : `${base.replace(/\/$/, '')}/${name}`;
    try {
      const ok = await write(machine, 'mkdir', { path: target });
      choose(ok?.path ?? target);
    } catch (e) {
      oops((e as Error).message);
    }
  }

  const crumbs = list ? trail(list.path, list.home) : [];
  /** `~` for the computer's own home, whatever it is called there. */
  const home = (p: string) => (list?.home && (p === list.home || p.startsWith(`${list.home}/`)) ? `~${p.slice(list.home.length)}` : tilde(p));
  const here = picked?.path ?? list?.path;
  const recent = (list?.recent ?? []).slice(0, 6);
  const spaceHere = here ? m?.spaceDirs?.[here] : undefined;

  return (
    <View style={{ flex: 1, backgroundColor: t.bg }}>
      <Nav
        big
        title="选择目录"
        sub={
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 5 }}>
            <View style={[s.dot, { backgroundColor: online ? t.ok : t.dim }]} />
            <Text style={{ fontSize: 13, color: t.dim }}>{m?.name ?? '电脑'}</Text>
          </View>
        }
      />
      {!online ? (
        <Notice text={m?.error ?? '电脑没连上'} />
      ) : !can ? (
        <Notice text={m.role === 'full' ? NOT_YET : '只有「完全控制」权限的手机能浏览电脑上的目录'} />
      ) : (
        <>
          <ScrollView contentContainerStyle={{ paddingHorizontal: 18, gap: 12, paddingBottom: 16 }} keyboardShouldPersistTaps="handled">
            {!query && list && (
              <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={{ alignItems: 'center' }}>
                {crumbs.map((c, i) => {
                  const last = i === crumbs.length - 1;
                  return (
                    <Text key={c.path} style={{ fontFamily: mono, fontSize: 14, color: last ? t.text : t.dim, fontWeight: last ? '600' : '400' }}>
                      {i > 0 && <Text style={{ color: t.dim }}>{' / '}</Text>}
                      <Text onPress={last ? undefined : () => setPath(c.path)}>{c.label}</Text>
                    </Text>
                  );
                })}
              </ScrollView>
            )}
            <View style={[s.search, { backgroundColor: t.card, borderColor: query ? t.focus : t.line }]}>
              <Icon name="search" size={16} width={2} color={t.dim} />
              <TextInput
                value={query}
                onChangeText={setQuery}
                placeholder="输入路径，或搜文件夹名"
                placeholderTextColor={t.dim}
                autoCapitalize="none"
                autoCorrect={false}
                style={[{ flex: 1, fontSize: 16, color: t.text, fontFamily: query ? mono : undefined }, noOutline]}
              />
              {!!query && (
                <Pressable onPress={() => setQuery('')} style={[s.clear, { backgroundColor: t.well }]} accessibilityLabel="清空">
                  <Icon name="close" size={12} width={2.4} color={t.text} />
                </Pressable>
              )}
            </View>
            {!!error && <Notice tone="danger" text={error} />}
            {!query && recent.length > 0 && (
              <>
                <Label>最近用过</Label>
                <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
                  {recent.map((p) => (
                    <Pressable key={p} onPress={() => setPath(p)} style={[s.chip, { borderColor: t.line }]}>
                      <Text style={{ color: t.text, fontSize: 14 }}>{p.split('/').filter(Boolean).pop()}</Text>
                    </Pressable>
                  ))}
                </View>
              </>
            )}
            {!query && list && (
              <>
                <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
                  <Label>
                    {crumbs.at(-1)?.label ?? '~'} 里的文件夹 · {list.dirs.length}
                  </Label>
                  {busy && <ActivityIndicator size="small" />}
                </View>
                <View style={[s.list, { borderColor: t.line, backgroundColor: t.card }]}>
                  {list.dirs.length === 0 && <Notice text="这里没有子文件夹" />}
                  {list.dirs.map((d, i) => {
                    const on = picked?.path === d.path;
                    return (
                      <View key={d.path} style={[s.li, { borderColor: t.sep }, i === 0 && { borderTopWidth: 0 }, on && { backgroundColor: t.dark ? '#121c2e' : '#eef3ff' }]}>
                        <Pressable style={s.liMain} onPress={() => setPicked(on ? null : d)}>
                          <Icon name="folder" size={20} color={on ? t.link : t.dim} />
                          <View style={{ flex: 1, gap: 2 }}>
                            <Text numberOfLines={1} style={{ fontSize: 16, color: t.text }}>
                              {d.name}
                            </Text>
                            <Text numberOfLines={1} style={{ fontSize: 12, color: t.dim }}>
                              {d.git ? (
                                <>
                                  <Text style={{ fontFamily: mono }}>{d.git.branch || 'git'}</Text>
                                  {changes(d.git) ? ` · ${changes(d.git)}` : ''}
                                </>
                              ) : (
                                '普通文件夹'
                              )}
                              {!!d.sessions && <Text style={{ color: t.run }}>{` · ${d.sessions} 个会话`}</Text>}
                            </Text>
                          </View>
                          {on && (
                            <View style={[s.tick, { backgroundColor: t.link }]}>
                              <Icon name="check" size={14} width={3} color={t.bg} />
                            </View>
                          )}
                        </Pressable>
                        <Pressable onPress={() => setPath(d.path)} style={[s.enter, { borderColor: t.sep }]} accessibilityLabel={`打开 ${d.name}`}>
                          <Icon name="right" size={14} width={2} color={t.dim} />
                        </Pressable>
                      </View>
                    );
                  })}
                </View>
              </>
            )}
            {!list && !error && !query && <ActivityIndicator style={{ marginTop: 30 }} />}
            {!!query && !typedPath && (
              <>
                <Label>
                  家目录下名字里有「{query.trim()}」的文件夹{found ? ` · ${found.length}` : ''}
                </Label>
                {!found ? (
                  <ActivityIndicator />
                ) : (
                  <View style={[s.list, { borderColor: t.line, backgroundColor: t.card }]}>
                    {found.length === 0 && <Notice text="没找到" />}
                    {found.map((d, i) => (
                      <Pressable key={d.path} onPress={() => choose(d.path, d)} style={[s.found, { borderColor: t.sep }, i === 0 && { borderTopWidth: 0 }]}>
                        <Text numberOfLines={1} style={{ fontFamily: mono, fontSize: 15, color: t.text }}>
                          <Mark text={tilde(d.path)} query={query.trim()} t={t} />
                        </Text>
                        <Text numberOfLines={1} style={{ fontSize: 12, color: t.dim }}>
                          {[d.git ? 'git' : '', about(d), d.sessions ? `${d.sessions} 个会话` : '', d.recent ? '最近用过' : ''].filter(Boolean).join(' · ')}
                        </Text>
                      </Pressable>
                    ))}
                  </View>
                )}
                {!!found && !!cut && <Notice text={cut} />}
              </>
            )}
            {!!query && typedPath && (
              <>
                <Label>直接用这个路径</Label>
                <Pressable onPress={() => choose(query.trim())} style={[s.found, s.list, { borderColor: t.line, backgroundColor: t.card }]}>
                  <Text style={{ fontFamily: mono, fontSize: 15, color: t.text }}>{query.trim()}</Text>
                  <Text style={{ fontSize: 13, color: t.dim, lineHeight: 19 }}>以 ~ 开头的路径按电脑上的家目录展开。路径不存在时用下面的「新建文件夹」。</Text>
                </Pressable>
              </>
            )}
            {!!query && (
              <View style={[s.lockNote, { borderColor: t.line }]}>
                <Icon name="lock" size={16} width={2} color={t.dim} />
                <Text style={{ flex: 1, fontSize: 13, color: t.dim, lineHeight: 19 }}>
                  只列文件夹，不读文件内容；跳过 node_modules、.git 和隐藏文件夹。只有「完全控制」权限的手机能浏览和开新任务。
                </Text>
              </View>
            )}
          </ScrollView>
          {!query && <View style={[s.foot, { borderColor: t.sep, paddingBottom: bottom + 4 }]}>
            <Wide
              disabled={!here}
              label={here ? `在 ${home(here)} 开始` : '选一个文件夹'}
              cut="head"
              onPress={() => here && choose(here, picked)}
              textStyle={{ fontFamily: mono, fontSize: 15 }}
            />
            <View style={{ flexDirection: 'row', gap: 8 }}>
              <View style={{ flex: 1 }}>
                <Wide kind="grey" height={40} label="新建文件夹" disabled={!supports(m, 'mkdir')} onPress={() => setMaking(true)} textStyle={{ fontSize: 15 }} />
              </View>
              <View style={{ flex: 1 }}>
                <Wide
                  kind="grey"
                  height={40}
                  label={spaceHere ? `空间 ${spaceHere}` : adding ? '正在设置…' : '设为空间'}
                  disabled={!here || !!spaceHere || adding || !supports(m, 'addspace')}
                  onPress={() => here && makeSpace(here)}
                  textStyle={{ fontSize: 15 }}
                />
              </View>
            </View>
          </View>}
        </>
      )}
      <SayDialog
        visible={making}
        name
        title={`在 ${list ? home(list.path) : '~'} 里新建文件夹`}
        placeholder={typedPath ? query.trim() : '文件夹名，或者完整路径'}
        action="新建并在这里开始"
        onClose={() => setMaking(false)}
        onSend={(name) => {
          setMaking(false);
          mkdir(name);
        }}
      />
    </View>
  );
}

const s = StyleSheet.create({
  dot: { width: 7, height: 7, borderRadius: 4 },
  search: { height: 48, borderRadius: 14, borderWidth: 1, flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 14 },
  clear: { width: 28, height: 28, borderRadius: 14, alignItems: 'center', justifyContent: 'center' },
  chip: { height: 36, borderRadius: 18, borderWidth: 1, paddingHorizontal: 14, justifyContent: 'center' },
  list: { borderWidth: 1, borderRadius: 16, overflow: 'hidden' },
  li: { flexDirection: 'row', alignItems: 'stretch', borderTopWidth: 1 },
  liMain: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 12, paddingLeft: 14, paddingRight: 10, minHeight: 64 },
  tick: { width: 22, height: 22, borderRadius: 11, alignItems: 'center', justifyContent: 'center' },
  enter: { width: 48, alignItems: 'center', justifyContent: 'center', borderLeftWidth: 1 },
  found: { gap: 4, paddingVertical: 12, paddingHorizontal: 14, borderTopWidth: 1 },
  lockNote: { flexDirection: 'row', gap: 10, borderWidth: 1, borderStyle: 'dashed', borderRadius: 16, padding: 14 },
  foot: { borderTopWidth: 1, paddingHorizontal: 18, paddingTop: 12, gap: 8 },
});
