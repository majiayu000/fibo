---
status: accepted
date: 2026-11-17
decision-makers: majiayu000
---

# 置顶跟着会话所在的电脑走：存在 host 里

## Context

用户两台 Mac：本机的窗口通过 ssh 连到 agentos 的 host 看它的会话。在 agentos 那台窗口里置顶的会话，在本机看 agentos 时不在「置顶」里。原因：置顶只存在各自窗口的 `<config>/sidebar`（按终端 id 记），host 不知道它。

## Decision

置顶归会话所在那台的 host 管：

- `Workspace::pinned_terminals: Option<Vec<String>>`，按置顶顺序记终端 id；host 存在 sqlite 的 `pinned_terminals` 表，随工作区（含增量 `WorkspaceDelta`）下发。
- 改它用 `Edit::PinTerminal { id, pinned }`。终端结束后 pin 不立即删，下一次改置顶时顺手去掉；列表只画还在跑的终端，所以看不到。
- 窗口看别的电脑时，「置顶」读那台的 host；远端行右键「置顶 / 取消置顶」经那台的 workbench 发给那台的 host。会话优先（含远端混排、只看一台）和「分台」布局都把那台的置顶行放进「置顶」。
- `<config>/sidebar` 里已有的 `pin` 行：本机窗口第一次连上会存置顶的 host 时，把还在跑的那些发过去，然后从文件里删掉。

不做：手机上显示置顶；别的电脑「先不管」时的灰行里不分置顶。

## Consequences

兼容（CLAUDE.md「换了版本，旧的东西还在」）：

- 新窗口连旧 host：旧 host 的工作区没有这个字段，读成 `None`，窗口照旧用本机文件；看别的电脑而那台 host 旧，远端行不给右键菜单（没处存）。不会往旧 host 发它不认识的 `PinTerminal`。
- 旧窗口连新 host：多出来的字段被忽略，旧窗口照旧用本机文件；新 host 存的置顶它看不到，它的置顶 host 也不知道。
- 迁移后退回旧版窗口：本机文件里的 pin 已经搬走，旧窗口里置顶是空的，要重新置顶一次。
- 顺带：别的电脑的 workbench 以前也读写本机 `<config>/sidebar`，按代码看会拿那台的工作区去剪本机的 pin 并写回文件（没在真机上复现过）。现在它不碰本机文件里的置顶。

## Implementation Plan

- core：`crates/fibo-core/src/model/pins.rs`（规则和测试）、`model.rs`、`model/edit.rs`。
- host：`crates/fibo-host/src/store.rs`。
- 窗口：`workbench/sidebar_list/pins.rs`（读哪份、改哪份、迁移）、`sidebar_list/session_list.rs`、`sidebar_recent.rs`、`sidebar_machine_views.rs`、`sidebar_machines/remote_menu.rs`、`hosts.rs`（`View::pins`）。
- 不要再直接读 `self.sidebar.pinned`；读 `self.pins()`。

## Verification

- `cargo test -p fibo-core --lib pins`、`cargo test -p fibo-host --lib pins_survive`
- `bash scripts/hosts-evidence.sh`：日志里有 `hosts pins PASS`（本窗口在 far 的行上置顶，far 的 host 存下；far 那边置顶或取消，本窗口侧栏跟着变）。
- `MOORING_PEEK_EVIDENCE`：本机行菜单置顶后 host 存下，终端结束后不再列出。

## Alternatives

- 置顶留在窗口里、按 host 分文件存：另一台自己的窗口仍然看不到，不解决问题。
- 新 host 判断用版本号：`Option` 字段本身就说明 host 认不认识它，不用另加版本协商。
