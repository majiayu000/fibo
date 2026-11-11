// Pairing: scan the QR code the computer shows, or paste its link. Scanning it with the
// phone's own camera opens this screen too, since the app owns the `mooring://` scheme.
import { CameraView, useCameraPermissions } from 'expo-camera';
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { Platform, StyleSheet, Text, TextInput, View } from 'react-native';
import { tr } from '@/i18n';
import { pair } from '@/remote/hub';
import { escape, type Link, parseLink } from '@/remote/link';
import { Nav, TITLE_WEIGHT, Wide, useBottom } from '@/ui/kit';
import { useTheme } from '@/ui/theme';

export default function Pair() {
  const t = useTheme();
  const bottom = useBottom();
  const [pasting, setPasting] = useState(Platform.OS === 'web');
  const params = useLocalSearchParams<Record<string, string>>();
  const [permission, requestPermission] = useCameraPermissions();
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const done = useRef(false);
  // A scanned or opened link waits here until the person says yes (plan stage 4): a
  // link someone else sent never pairs this phone on its own.
  const [candidate, setCandidate] = useState<Link | null>(null);

  function use(linkText: string) {
    if (done.current || busy || candidate) return;
    try {
      setCandidate(parseLink(linkText));
      setError('');
    } catch (e) {
      setError(tr('这不是 Fibo 的配对链接（{reason}）', { reason: (e as Error).message }));
    }
  }

  async function confirm(link: Link) {
    if (done.current || busy) return;
    done.current = true;
    setBusy(true);
    try {
      await pair(link);
      router.dismissTo('/');
    } catch (e) {
      done.current = false;
      setCandidate(null);
      setError(tr('没配上：{reason}', { reason: (e as Error).message }));
    } finally {
      setBusy(false);
    }
  }

  // Opened from mooring://pair?…: the link arrives as this route's params.
  useEffect(() => {
    if (params.code && params.key) {
      const query = Object.entries(params)
        .map(([k, v]) => `${k}=${escape(String(v))}`)
        .join('&');
      use(`mooring://pair?${query}`);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params.code, params.key]);

  if (candidate) {
    const relay = candidate.relay.replace(/^\w+:\/\//, '');
    return (
      <View style={[s.page, { backgroundColor: t.bg }]}>
        <Nav big title={tr('配对电脑')} />
        <View style={s.body}>
          <Text style={{ color: t.text, fontSize: 19, fontWeight: TITLE_WEIGHT }}>{tr('和「{name}」配对？', { name: candidate.name || tr('这台电脑') })}</Text>
          <Text style={[s.hint, { color: t.dim }]}>
            {tr('经过中继 {relay}。只在你刚在自己电脑的 Fibo 里点了「配对新手机」时才确认；别人发来的链接不要配。', { relay })}
          </Text>
        </View>
        <View style={[s.foot, { paddingBottom: bottom + 4 }]}>
          <Wide label={busy ? tr('正在连接…') : tr('配对')} disabled={busy} onPress={() => confirm(candidate)} />
          <Wide label={tr('取消')} kind="grey" disabled={busy} onPress={() => setCandidate(null)} />
        </View>
      </View>
    );
  }

  // The bold 「这里」 sits inside the sentence, wherever the language puts it.
  const stepThree = tr('3. 用{here}对准它。微信和系统相机打不开这种码').split('{here}');
  return (
    <View style={[s.page, { backgroundColor: t.bg }]}>
      <Nav big title={tr('配对电脑')} />
      <View style={s.body}>
        <View style={[s.camera, { borderColor: t.line, backgroundColor: t.card }]}>
          {permission?.granted ? (
            <CameraView
              style={StyleSheet.absoluteFill}
              facing="back"
              barcodeScannerSettings={{ barcodeTypes: ['qr'] }}
              onBarcodeScanned={busy ? undefined : (r) => use(r.data)}
            />
          ) : (
            Platform.OS !== 'web' && (
              <View style={s.center}>
                <Wide label={tr('允许使用相机来扫码')} kind="grey" height={44} onPress={requestPermission} />
              </View>
            )
          )}
          <View pointerEvents="none" style={s.frame}>
            {(['tl', 'tr', 'bl', 'br'] as const).map((c) => (
              <View
                key={c}
                style={[
                  s.corner,
                  { borderColor: permission?.granted ? '#fff' : t.text },
                  c[0] === 't' ? { top: 0, borderTopWidth: 3 } : { bottom: 0, borderBottomWidth: 3 },
                  c[1] === 'l' ? { left: 0, borderLeftWidth: 3 } : { right: 0, borderRightWidth: 3 },
                  { [`border${c[0] === 't' ? 'Top' : 'Bottom'}${c[1] === 'l' ? 'Left' : 'Right'}Radius`]: 12 },
                ]}
              />
            ))}
          </View>
        </View>
        <View style={{ gap: 10 }}>
          <Text style={[s.step, { color: t.dim }]}>{tr('1. 在电脑的 Fibo 里点标题栏的手机图标')}</Text>
          <Text style={[s.step, { color: t.dim }]}>{tr('2. 选「配对新手机」，屏幕上会出现二维码')}</Text>
          <Text style={[s.step, { color: t.dim }]}>
            {stepThree[0]}
            <Text style={{ color: t.text, fontWeight: '700' }}>{tr('这里')}</Text>
            {stepThree[1]}
          </Text>
        </View>
        {pasting && (
          <TextInput
            value={text}
            onChangeText={(v) => {
              setText(v);
              setError('');
            }}
            placeholder="mooring://pair?…"
            placeholderTextColor={t.dim}
            autoCapitalize="none"
            autoCorrect={false}
            style={[s.input, { color: t.text, borderColor: t.line, backgroundColor: t.card }]}
          />
        )}
        {!!error && <Text style={{ color: t.err }}>{error}</Text>}
      </View>
      <View style={[s.foot, { paddingBottom: bottom + 4 }]}>
        <Wide
          kind="grey"
          label={pasting ? tr('下一步') : tr('粘贴配对链接')}
          disabled={pasting && (!text || busy)}
          onPress={() => (pasting ? use(text) : setPasting(true))}
        />
        <Text style={{ textAlign: 'center', fontSize: 12, color: t.dim, lineHeight: 18 }}>
          {tr('手机和电脑最好在同一个 Wi‑Fi；不在时经中继连接，内容端到端加密')}
        </Text>
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  page: { flex: 1 },
  body: { flex: 1, paddingHorizontal: 18, paddingTop: 8, gap: 20 },
  hint: { fontSize: 14, lineHeight: 21 },
  camera: { aspectRatio: 1.05, borderRadius: 22, overflow: 'hidden', borderWidth: 1 },
  center: { position: "absolute", left: 0, right: 0, top: 0, bottom: 0, alignItems: 'center', justifyContent: 'flex-end', padding: 20 },
  frame: { position: 'absolute', left: '22%', right: '22%', top: '19%', bottom: '25%' },
  corner: { position: 'absolute', width: 34, height: 34 },
  step: { fontSize: 15, lineHeight: 22 },
  input: { borderWidth: 1, borderRadius: 12, padding: 12, fontSize: 14 },
  foot: { paddingHorizontal: 18, gap: 10 },
});
