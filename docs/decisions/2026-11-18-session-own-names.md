---
status: accepted
date: 2026-11-18
decision-makers: majiayu000
---

# 会话用自己的名字，待办名留给标签；分屏不跟标签的待办

## Context

11-18 用户看侧栏：「怎么是一个tab的1234啊 不能做成 每个会话自己的命名吗」。「agent纵览」标签里四块分屏在侧栏叫「agent纵览 · Codex」「agent纵览 · Claude Code」「agent纵览 · Codex 2」「agent纵览 · Codex 3」。

两个原因叠在一起：

1. 名字顺序是「你起的名 → 待办标题 → CLI 起的名 → 第一句话」（`session_naming`）。待办下有几场对话就都叫待办名，11-17 的 721839a9 为不重名补了「· Agent N」。
2. 对话第一次绑定终端时，没登记待办就拿所在标签的默认待办（`workspace_edit.rs`）。在待办标签里分屏开的任何对话都算这个待办的。

本机库里三个待办（agent纵览、搜索工具、fibo纵览）下 11 场对话，每场 CLI 都起了互不相同的名字（「审视项目进展与当前目标」「SkyFable 计费剩余工作」……）；「搜索工具」下五场只有一场跟搜索有关。

## Decision

- 名字顺序改为：你起的名 → CLI 起的名 → 第一句话 → 待办标题 → 「CLI · 目录」。同一个待办下两场算出来的名字完全一样时，才补「· Agent」「· Agent N」（`task_names.rs`，判重从「同待办」改成「同待办且同名」）。
- 标签照旧：一个标签里几场同属一个待办时，标签叫待办名。标签管一组，侧栏每行各叫各的。
- 分屏放进带默认待办的标签时，新终端登记「不关联待办」（`pending_tasks` 记 `None`，和「不关联待办」按钮同一个状态）。从待办「再开一个」时，窗口在放进标签前先登记这个待办（`Store::launch` 的 `task`）。标签打开时那块画面、在原画面里重开的 Agent 照旧跟标签的待办。

不做：已经挂错待办的旧对话不批量摘下（名字已经各叫各的，摘不摘要用户定）。

## Consequences

- 名字读时现算，不存盘：换包后已有对话直接显示新名字。旧版 host 服务的手机仍是旧名字，直到 host 换新。
- 刚从待办开的对话第一句话是交接文字，开头就是待办标题，所以 CLI 起名前显示的还是待办名。
- Claude 的 ai-title 偶尔被它自己重新生成，名字可能变一次。
- 新窗口 + 旧 host：旧 host 的 `SplitPane` 不登记 `None`，分屏照旧跟待办。旧窗口 + 新 host：从待办「再开一个」不先登记，新开的那场不挂待办，要手动关联。窗口和 host 同包发，只在换包的间隙出现。
- `fibo open --beside`（Agent 在自己旁边开终端）进待办标签时也不再跟待办。

## Verification

```
cargo test -p fibo-core task_names_tests split_task_tests session_names_prefer
cargo test -p fibo-core --test one_owner
python3 scripts/visual-check.py shots <sha> <out> --pages Main@1420,Overview@1420
```

## Alternatives

- 只在一个待办挂多场时才用自己的名字：第二场一开，第一场的名字会从待办名跳成别的。
- 待办标题排在第一句话前面：没有 CLI 名的 Agent（Gemini、Kimi 等）在同一待办下又回到「待办 · Agent N」；从待办开的对话第一句话本来就以待办标题开头，不需要这样排。
- 侧栏行加一行小字写待办：用户 11-06 说过侧栏不能动。

## More Information

- 取代 721839a9 的命名顺序（防重名保留，判重改成按名字）。
- 设计页：`<本机证据目录>/fibo/2026-11-18-session-names/index.html`。
- 重开条件：用一周后，如果更常靠「哪个待办」而不是「在干什么」找对话，退回「待办名 · CLI 名」。
