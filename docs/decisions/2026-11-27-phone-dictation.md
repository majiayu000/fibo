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
- 电脑读出的中文里夹着的半角 `,` `?` `!` `:` `;` 换成全角。

## Consequences

- 安卓多了 `RECORD_AUDIO`。以前 `expo-image-picker` 的 `microphonePermission: false` 会把它列进屏蔽权限，`expo-camera` 的同一项会删掉 iOS 的 `NSMicrophoneUsageDescription`；两处都改成了同一句说明，相机仍不录音（`recordAudioAndroid: false`）。
- 原生模块：要重新打包 App，不是 JS 热更新能带上的。
- 识别在不在、准不准，看手机厂商装了什么服务；国内安卓手机没有 Google 服务时，用的是厂商自己的。

## Implementation Plan

- `mobile/src/ui/dictation.tsx`：`useDictation` 和 `MicButton`；整台手机一个识别器，只有开始它的那个输入框收事件。
- `mobile/src/ui/spoken.ts`：把听到的字接到原文后面（中文不加空格，英文单词之间加）。

## Verification

```sh
cd mobile && bun test test/spoken.test.ts && bun run typecheck && bun run lint
bunx expo config --type introspect --json | grep -E 'NSMicrophoneUsageDescription|RECORD_AUDIO'
```

真机：`docs/phone-app/walkthrough.md` 的 `chat.voice`。

## Alternatives

- 继续只靠输入法语音键：用户这次明确要 App 里有。
- 录音发到电脑或云端再转文字：多一条传音频的路和隐私面，用户说用手机自带的就行。

## More Information

重开条件：真机上厂商识别服务普遍不可用，或者用户要离线 / 更准的识别。
