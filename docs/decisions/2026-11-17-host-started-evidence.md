---
status: accepted
date: 2026-11-17
decision-makers: 用户（「把 v0.1.3 发完」）、Claude
---

# 后台自己交给 Agent 的第一句和要接着的对话，是认对话的证据

## Context

RC3 隔离桌面验收（`~/Documents/claude-pages/fibo/2026-11-17-release-v0.1.3/`）查出两条阻断，复核员确认：

- 手机新任务、待办「开始」把任务放在命令行上交给 CLI，没有回车。认对话只剩「画面上原样出现任务文字」：`screen_key` 去掉空白后要求它在画面上连续出现。OpenCode 1.19.5 在任务框每一行（折行也算）前画「┃」，Codex 0.169 把 `代码` 的反引号渲染掉，于是多行、折行、带反引号的任务一直对不上，手机上没有状态、聊天和审批卡。
- OpenCode 留空接着做（`opencode --session ses_…`）在被说话之前不写日志，也没有回车，没有任何东西去找这个进程；OpenCode 的命令行又不当会话证据（[2026-11-07-agent-regression](2026-11-07-agent-regression.md)）。终端对不上，手机聊天因为没有对话而拒发，接着做需要的第一句永远发不出去。

同一个入口在修复前的后台（origin/main `ececaa43`，库和协议与 RC3 相同）上用真 CLI 走过一遍（`rc4/fix/before-gaps/`、`rc4/fix/old-data/`）：Claude Code 两行、折行、反引号和留空接着做都由 Hook 对上，没有这两个缺口；Codex 两行、折行靠画面对上，带反引号的两行任务两分钟都对不上，留空接着做靠命令行里的会话 ID 对上，但要等 37 秒才有东西去看进程；OpenCode 每一种都对不上。

要支撑的决定：手机开的终端能不能从一开始就有状态、聊天和审批卡；判定是三家 CLI 的真实进程在隔离后台上都对上，而且同一句任务开在同一目录的两块终端谁也不绑到对方。

## Decision

后台开终端时记下自己交给 Agent 的东西（`Request::Launch.start`：Agent、命令行交出的第一句、要接着的对话的 CLI 会话 ID），存在 `Workspace::agent_starts`（单独的 `agent_starts` 表），终端第一次对上对话或结束时收掉（`Workspace::settle_starts`）。用它的两条规则在 `crates/fibo-host/src/recognition/launched.rs`：

1. **第一句**：同一 CLI、同一目录、不早于开终端那一秒开始的对话，第一条用户消息（两边都按 `content::user_text` 去掉引用和代码块，再去掉空白）和交出的完全一样。它和画面、回车证据在同一个一对一规则里（`screen_bindings`）：一条对话配上两块，或一块配上两条，都不绑。回车比第一句新：一段对话后来的一句在某块里按了回车，它就不再按第一句配；一块里回车发的不是开它时交出的那句（另打了一句、`/new`、换了会话），这块也不再按第一句配。这样同一句任务开的两块，在其中一块打下一句时就分开了；为开头那句按的回车（答信任框正赶上任务写进日志）不算更新的话。一块按开它时交出的那句开起来、画面上画着那句，这是开它那件事本身的证据，不另算画面证据：Codex 把两行任务原样画出来，不这样的话两块都显示着对方对话的那句，打字也分不开；早于开它就开始的旧对话，第一句碰巧一样，也不会因为这块画着同样的字被配上。
2. **接着的对话**：那块终端里只有一个该 CLI 的进程、它的命令行没写别的对话、那段对话的日志已知且不在别处运行，就直接对上。

带 `start` 的终端开出来后 `host/launch_probe.rs` 退避着看几次进程（约一分钟，见到 Agent 就停），因为没有回车也没有日志会触发探测。

不做：

- 不加新的 `Binding` 枚举值。RC3 的后台读到不认识的值整库读不出（`store` 的 `enum_value`），窗口也解析不了工作区；三种对上都记成 `Binding::Screen`，它的意思放宽成「进了 TUI 的东西」：画面、回车、或后台交给它的第一句和对话。好处是以后 `/new`、换会话时下一句照样能把窗格挪走，后台重启后靠窗格里唯一的 CLI 认回进程（`only_cli_of_screen_bound`）。
- 不放宽画面匹配（去「┃」、忽略反引号之类）：仍是猜画面。
- 不从命令行解析 OpenCode 的 `--session`，也不从已存的 `Terminal.argv` 反推任务：前者违背上面那条决定且每次探测都会重新认定，后者依赖 `run_then_stay` 的形状，Windows 又不同。
- 不让手机在没对上对话时也发：会打进 shell 或别的对话。
- 不靠开始时间先后去分同一句任务的两块终端：只能猜。
- 不让第一句和回车一样重、一并比（上一版）：同任务的两块永远分不开，在电脑上打字也不行，比修复前还差。也不让任何回车都作废 start：答信任框的回车会让 Codex 新目录里的任务又对不上。
- 不把 OpenCode 的 `input_hint` 放宽成也认界面底栏，用它挡接着做的对上：这个提示词还管 `fibo run` 和待办「开始」什么时候交任务，它的测试要求权限框开着时不算准备好；权限框开着时底栏在不在没有实测，放宽了就可能把任务打进权限框，还要改插件版本和目录签名。也不按「括号粘贴开了」算准备好：实测开着它照样丢字。

## Consequences

- 两块终端在同一目录用同一句任务开，两块都不绑，直到在电脑上往其中一块打下一句：那块按回车对上自己的对话，另一块只剩另一段，也对上。手机上在这之前只能看终端打字。只按「谁的第一句」合并成一个规则（不让回车压过第一句）时，这两块会一直对不上，连在电脑上打字也分不开，比修复前还差，所以分了先后。
- 回车打在离另一块那句任务的日志时间三秒以内，回车也算那段对话的证据，两块照旧都不绑（老规则，`SUBMIT_WINDOW`）。
- OpenCode 留空接着做时，进程一出现就对上（实测开出后 2.3–2.6 秒），而它的界面要再过约一秒才画出来；这一秒里打进去的字它自己丢掉，后台照常回复成功（`rc4/fix/r2/opencode-ready*/`：开出后 2.9 秒、括号粘贴已开着的一句也丢了；界面画出来之后马上打的字立刻出现在输入框）。手机上从点「接着做」到打完一句通常远不止这一秒，但没有挡住。没做的办法见「不做」。
- 老后台起的终端没有 `start`，行为和以前一样；旧库多建一张空表照常读出；RC3 不认识这张表也不碰它。`Launch.start` 旧后台会忽略。
- OpenCode 的记录只留 4000 字：第一句超过时，按留下的开头（至少 1000 字）比。
- 记成 `Screen` 的对上，窗口卡片的说明是「画面上的提示词，或打开它时给它的任务、对话」。

## Implementation Plan

- `crates/fibo-core/src/model/agent_start.rs`、`protocol.rs`（`Launch.start`）、`model.rs`（`agent_starts`）。
- `crates/fibo-host/src/store/starts.rs`；`host/client.rs`（记下、开探测）、`host/launch_probe.rs`、`host/status.rs`（`refresh_agents` 里接着的对话）、`host/screen_match.rs` 和 `host/association.rs`（第一句）。
- `crates/fibo-host/src/remote/writes.rs`（手机三种 open）、`crates/fibo-app/src/workbench/launch_slot.rs`（窗口的接着做）。
- 新开终端的入口要带上 `start`，不带就是没有这条证据，不是错。

## Verification

```sh
cargo test -p fibo-host --lib -- recognition::launched_tests store::starts
cargo test -p fibo-host --test managed_session started::
cargo test -p fibo-host --test declared_agent a_declared_cli_the_host_resumed
cargo test -p fibo-host --test remote_open_start
cargo test -p fibo-core --lib agent_start
```

逐项撤回（证据关掉、回车不压过第一句、画着自己那句算画面证据、接着做关掉、探测关掉、手机不带 start、不存、不收）时对应测试都失败，记录在 `rc4/fix/undo/` 和 `rc4/fix/undo2/`。真实 CLI 的检查见同目录。

## Alternatives

见 Decision 里的「不做」，以及提交说明的 `Rejected:`。

## More Information

重开条件：某个 CLI 的日志不再原样保存第一条用户消息（改写、截断得更短），或者出现第一句相同、目录相同、却由 Fibo 管不到的进程同时开始的真实案例。来源：RC3 记录 `v0.1.3-rc3.md` 的阻断 1、2 和 Codex 反引号那条相邻项。
