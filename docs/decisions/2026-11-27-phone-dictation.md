---
status: accepted
date: 2026-11-27
decision-makers: 用户
---

# 手机输入框加麦克风，用系统自带的语音识别

## Context

11-06 收件箱上真机时定过「不做 App 内语音」：搜狗输入法的语音键在真机上能用（`docs/phone-app/v2-plan.md`）。11-27 用户要求：「给 fibo 手机端增加语音输入的功能……可以使用手机自带的语音转换」。输入法的语音键要先换到那个输入法、再找按键，离开电脑时一手拿手机说一句话太绕。

## Decision

- 聊天输入框（`mobile/src/ui/composer.tsx`）发送键左边一个麦克风，新任务页（`mobile/src/app/new.tsx`）一个「说话输入」按钮。点一下开始，再点一下结束；听到的字边说边接在原来写的字后面，**不自动发送**。
- 识别用系统的：安卓 `SpeechRecognizer`、iOS `SFSpeechRecognizer`，经 `expo-speech-recognition`。不接云端识别服务，不录音存文件。
- 手机没有识别服务（`isRecognitionAvailable()` 为假）就不显示麦克风，输入法的语音键照旧可用。
- 打字、发送、开始任务、离开页面都会立刻停掉识别，之后到的字不再写进框里。
- 识别语言跟 App 的界面语言：中文界面 `zh-CN`，其他 `en-US`。

### 11-28 追加：手机识别不了时交给电脑

用户的 OPPO（ColorOS 16）系统不给 App 提供语音识别（没装 Google 的 Speech Recognition & Synthesis，小布不对外开放）。国内 App（微信、搜狗）都是自己录音、压缩、传到自家服务器识别。Fibo 的「自家服务器」就是用户的电脑，用户：「那你这样做吧……我觉得没有问题的」。

- 手机系统识别用不了、电脑在 `hello` 里列了 `transcribe` 时，麦克风改为录音：系统录音器录 16 kHz 单声道 AAC（.m4a，约 4 KB/秒，最长 90 秒），整段 base64 发给电脑。
- 电脑 `transcribe`（只给完全控制）：`ffmpeg` 转 WAV，`whisper-cli`（whisper.cpp）用运行目录 `models/` 里的 `ggml-large-v3-turbo-q5_0.bin` 识别，中文带简体和标点的提示；有 `ggml-silero-v5.1.2.bin` 就开 VAD 跳过没人说话的部分。录音不离开电脑，用完即删。三样缺一样就不列这个操作，手机也不显示麦克风。
- 实测（M2 Max）：11 秒的话 46 KB，说完到出字约 2 秒。

现在要手动装：`brew install whisper-cpp ffmpeg`，把两个模型放进运行目录的 `models/`。**下一步**：电脑 App 设置里一个开关负责下载模型和说明依赖（用户偏好：功能要能在设置里配，不能只靠命令行）。

### 11-28 追加：像微信一样，点一下变绿、边说边出字

先画了三个方向，用户选了底部弹出面板；真机上用户说：「不是这样的弹窗啊 这个看起来非常难用」「微信是 按一下 然后麦克风就绿了 然后 再按一下 就关了 绿的时候，说话是直接出字的」。面板撤掉，回到点一下开、再点一下停，加两点：

- 在听的时候麦克风是绿的（`t.allow`）。
- 交给电脑时也边说边出字：安卓录 ADTS 格式的 AAC（`.aac`，一帧一帧写，录到一半也读得了；MP4 要等录完写了索引才读得了），录着的时候每 1.8 秒把录到的整段发给电脑读一次，同一时间只发一个，读回来的字替换上一次的；停下后整段再读一次，作为定稿。iPhone 有自己的识别，不走这条。
- 手机系统识别只对谷歌的服务用连续模式（它接 App 交过去的录音流）；别家的服务自己录音、一句一句地听，听完一句自动接着听，直到再点麦克风。点了 3 秒还没回「开始听」的识别服务（vivo 的就这样，不报错也不出字）当作用不了，这次改成录音交给电脑，并直接开始录。
- 电脑 VAD 给人声前后各留 300 ms，不然停顿后第一个字会被裁掉。
- 电脑读出的中文里夹着的半角 `,` `?` `!` `:` `;` 换成全角。

### 11-28 再追加：照豆包的做，按住说话，松手发送（取代上面的「点一下变绿」）

用户看了点一下变绿的版本：「语音输入不是流式吗？……这个绿色就很奇怪」，要我学豆包的输入框：「甚至完全按照他的来做都是没有问题的」。画了几版（设计稿 F），用户定了：「可以的，没有问题。松手，直接发送」。

- 输入框空着时按住框（220 ms 以内算点一下，照常打字）：屏幕底部升起一片蓝光，里面是边说边出的字和跟着音量起伏的波形（`mobile/src/ui/holdtalk.tsx`）。上滑 80 px 变红，松手就丢掉。聊天里松手直接发送；新任务页松手放进「要它做什么」框，不开始任务。框里已有字时不接管按住，照常编辑。
- 跟豆包不同：说话时就显示字。Agent 会照着做，听错了最好松手前就看到。
- 交给电脑时不再每次重读整段录音（越说越慢）：ADTS 每帧自带长度，任意一串完整帧就是一个能读的文件（`mobile/src/ui/adts.ts`）。录音按停顿（600 ms）或最长 8 秒切成段，只重读正在说的这一段，每秒一次；段结束时读一次定稿留下。声音门限从 -35 降到 -42 dBFS，轻声说的一句以前整句没算进去。
- 手机系统识别 3 秒不回「开始听」就记下这个识别服务（按包名存在手机上）：以前只记在内存里，每次重开 App 的第一次按住都要先等 3 秒，这 3 秒说的字丢了（vivo 实测，「帮我把首页的标题改成蓝色，然后跑一下测试」只剩后半句）。换了默认识别服务会重新试。
- 实测（vivo V2352A，M2 Max 上 whisper）：每段读一次约 0.85 秒；开口后约 2 秒第一次出字，之后每秒更新。

## Consequences

- 安卓多了 `RECORD_AUDIO`。以前 `expo-image-picker` 的 `microphonePermission: false` 会把它列进屏蔽权限，`expo-camera` 的同一项会删掉 iOS 的 `NSMicrophoneUsageDescription`；两处都改成了同一句说明，相机仍不录音（`recordAudioAndroid: false`）。
- 原生模块：要重新打包 App，不是 JS 热更新能带上的。
- 识别在不在、准不准，看手机厂商装了什么服务；国内安卓手机没有 Google 服务时，用的是厂商自己的。

## Implementation Plan

- `mobile/src/ui/dictation.tsx`：`useDictation`；整台手机一个识别器，只有开始它的那个输入框收事件。
- `mobile/src/ui/holdtalk.tsx`：按住、上滑、松手（`useHold`）和底部的光（`VoiceGlow`）。
- `mobile/src/ui/voicememo.ts`、`adts.ts`：交给电脑时按段读。
- `mobile/src/ui/spoken.ts`：把听到的字接到原文后面（中文不加空格，英文单词之间加）。

## Verification

```sh
cd mobile && bun test test/spoken.test.ts test/adts.test.ts && bun run typecheck && bun run lint
bunx expo config --type introspect --json | grep -E 'NSMicrophoneUsageDescription|RECORD_AUDIO'
```

真机：`docs/phone-app/walkthrough.md` 的 `chat.voice`。

## Alternatives

- 继续只靠输入法语音键：用户这次明确要 App 里有。
- 录音发到电脑或云端再转文字：多一条传音频的路和隐私面，用户说用手机自带的就行。

## More Information

重开条件：真机上厂商识别服务普遍不可用，或者用户要离线 / 更准的识别。
