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
