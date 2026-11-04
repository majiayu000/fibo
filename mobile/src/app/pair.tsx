// Pairing: scan the QR code the computer shows, or paste its link. Scanning it with the
// phone's own camera opens this screen too, since the app owns the `mooring://` scheme.
import { CameraView, useCameraPermissions } from 'expo-camera';
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { pair } from '@/remote/hub';
import { escape, type Link, parseLink } from '@/remote/link';
import { useTheme } from '@/ui/theme';

export default function Pair() {
  const t = useTheme();
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
      setError(`这不是 Mooring 的配对链接（${(e as Error).message}）`);
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
      setError(`没配上：${(e as Error).message}`);
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
        <Text style={{ color: t.text, fontSize: 18 }}>
          和「{candidate.name || '这台电脑'}」配对？
        </Text>
        <Text style={[s.hint, { color: t.dim }]}>
          经过中继 {relay}。只在你刚在自己电脑的 Mooring 里点了「配对新手机」时才确认；别人发来的链接不要配。
        </Text>
        <Pressable
          disabled={busy}
          onPress={() => confirm(candidate)}
          style={[s.button, { backgroundColor: t.accent, opacity: busy ? 0.5 : 1 }]}
        >
          <Text style={{ color: t.onAccent, fontSize: 16 }}>{busy ? '正在连接…' : '配对'}</Text>
        </Pressable>
        <Pressable disabled={busy} onPress={() => setCandidate(null)} style={s.button}>
          <Text style={{ color: t.accent, fontSize: 16 }}>取消</Text>
        </Pressable>
      </View>
    );
  }

  return (
    <View style={[s.page, { backgroundColor: t.bg }]}>
      <Text style={[s.hint, { color: t.dim }]}>
        在电脑的 Mooring 里点标题栏的手机按钮，选「配对新手机…」，把二维码对准下面的框。
      </Text>
      <View style={[s.camera, { borderColor: t.line }]}>
        {permission?.granted ? (
          <CameraView
            style={StyleSheet.absoluteFill}
            facing="back"
            barcodeScannerSettings={{ barcodeTypes: ['qr'] }}
            onBarcodeScanned={busy ? undefined : (r) => use(r.data)}
          />
        ) : (
          <Pressable onPress={requestPermission} style={s.center}>
            <Text style={{ color: t.accent, fontSize: 16 }}>允许使用相机来扫码</Text>
          </Pressable>
        )}
      </View>
      <Text style={[s.hint, { color: t.dim }]}>或者粘贴电脑上「复制链接」得到的链接：</Text>
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
      <Pressable
        disabled={!text || busy}
        onPress={() => use(text)}
        style={[s.button, { backgroundColor: t.accent, opacity: !text || busy ? 0.5 : 1 }]}
      >
        <Text style={{ color: t.onAccent, fontSize: 16 }}>下一步</Text>
      </Pressable>
      {!!error && <Text style={{ color: t.danger }}>{error}</Text>}
    </View>
  );
}

const s = StyleSheet.create({
  page: { flex: 1, padding: 16, gap: 12 },
  hint: { fontSize: 14 },
  camera: {
    aspectRatio: 1,
    borderRadius: 16,
    overflow: 'hidden',
    borderWidth: StyleSheet.hairlineWidth,
  },
  center: { flex: 1, alignItems: 'center', justifyContent: 'center' },
  input: { borderWidth: 1, borderRadius: 10, padding: 12, fontSize: 14 },
  button: { alignItems: 'center', paddingVertical: 12, borderRadius: 10 },
});
