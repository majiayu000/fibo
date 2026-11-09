---
status: accepted
date: 2026-11-09
decision-makers: 用户（「修复吧」）、Claude
---

# 「后台连接断开」只说连接；识别状态不再借用它

## Context

窗口顶部的「后台连接断开，正在重新连接…」和底栏「后台断开」原来看的是识别状态 `RecognitionPhase::Interrupted`。这个状态有两个来源：窗口自己订阅状态的连接断了（`connection.rs` 造一个 Interrupted），以及后台自己报识别出了问题（补扫时发现有日志的目录没了、订阅文件事件失败）。后一种情况连接全都正常，窗口照样说断开。

同一个误报出现过两次：

- 11-08 `45dd219`：Windows 上没用过的 CLI 没有日志目录，后台每次启动都报 Interrupted。当时只修了后台（目录从没记录过日志就不算），窗口这边没动。
- 11-09：Mac 睡了 20 分钟，醒来 FSEvents 要求补扫，有个记录过会话的目录已被删，后台报「日志目录暂不可用，等待重建事件」并且永远停在这里（之后没有补扫会清它，每批事件还原样重发）。窗口一直显示断开，点「重新连接」也没用。

## Decision

1. 窗口把「连接断了」和「后台发来的状态」分成两种消息（`StatusLink::Lost` / `StatusLink::Status`）。只有 `Lost` 让 `Store::cut_off` 成立；收到任何状态，不管什么阶段，都说明后台答话了。
2. 后台报 Interrupted 而连接正常时，标题栏显示「会话识别中断」，原因在底栏的提示里，不弹断开横幅。
3. 补扫遇到记录过日志、现在已被删的目录：清掉它的记录后报 Healthy，说明里写「有记录过会话的日志目录被删除，记录已清理」。

不做：窗口按 detail 文字或 scopes 是否为空来猜是不是连接断了。

## Consequences

- 推翻 `45dd219` 的 Rejected「补扫结束一律报正常：真的被删的日志目录就没有提示了」：提示留在说明里，但不再是故障状态。被删的目录修完就是修完了，留着 Interrupted 没有任何东西能清掉它。
- 连接断开期间窗口保留上一份识别状态（原来会被一份空状态替换），重连后后台会发一份完整的新状态。

## Implementation Plan

- `crates/mooring-app/src/connection.rs`：`StatusLink`；`RecognitionConnection` 发它。
- `crates/mooring-app/src/host/store.rs`：`take_link`、`link_down`、`cut_off_reason`；`set_recognition` 只清断开。
- `crates/mooring-app/src/workbench/{mod,render,statusbar}.rs`、`desktop.rs`（菜单栏照旧把 Lost 当 Interrupted 看）。
- `crates/mooring-host/src/host/association.rs`：`recover_logs` 结尾。
- 别再写 `phase == Interrupted` 来判断连接。

## Verification

```sh
cargo test -p mooring-app --bins -- store_link
cargo test -p mooring-host --lib a_deleted_log_folder_ends_its_repair_healthy
```

两个测试在各自的改动撤回后都会失败（已验）。

## Alternatives

- 只修后台的卡死：下一个会报 Interrupted 的后台原因又会让窗口说断开，`45dd219` 就是这样只修了一半。
- 识别中断时完全不提示：后台真读不到日志时用户不知道为什么状态不准。

## More Information

重开条件：后台出现一种必须让用户重启后台才能恢复的识别故障，那时它该有自己的提示和按钮，仍不用「连接断开」。来源：本次会话 11-09 用户截图「是这里显示断开」。
