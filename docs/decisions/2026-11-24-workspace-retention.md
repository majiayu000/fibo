---
status: accepted
date: 2026-11-24
decision-makers: majiayu000
---
# 结束超过 30 天、没有事的会话移出 host 内存，库里留作归档

## Context

`terminals`、`agent_sessions` 只增不减，每次 commit 整份克隆和比较，成本随历史线性增长（11-19 一台长期安装有 831 个终端、内存里 5000 多条会话）。方案比较在 [workspace-retention/spec.md](../workspace-retention/spec.md)。

## Decision

用 spec 的方案 B，W = 30 天（用户 11-24 选「结束超 30 天归档」）。规则只在 `fibo_core::retention::archivable`：

- 会话留在内存，只要满足其一：它的终端还在标签里、被置顶、在跑或在等收尾；暂存（Deferred）；挂在没关的待办上；`session_updated` 在 30 天内；`needs_you`。
- 终端只和它的会话一起走：不在标签、不置顶、已结束、没有留下的会话指着它。没有会话的普通 shell 终端，结束且不在标签就走。
- **不删库**：只是不装入、不克隆、不发送。`agent_launch`、`session_names`、`outcome_history` 等表不动，内存里的这些表也不动（从内存表里删键会被 `save_applying` 当成删除）。
- 归档后日志又动了：任何入口新建的会话，在 `Host::commit_applying` 里按 `(runtime, cli_session_id)` 查库，有归档行就用原 id、原结局、名字、待办合并（`retention::readopt`），终端行一起读回。
- host 启动时装完内容就归档一次，之后每小时一次。

不做：删库（spec 方案 A）；一个「归档」视图（超过 30 天的从侧栏和手机消失，要找只能等它再动）。

## Consequences

- 窗口、手机看到的集合变了：绑过终端、结束超过 30 天、没有事的会话不再出现（原来绑过终端的永远在）。
- 启动仍从库读全部行和内容再归档；只有启动时这一次是全量。
- 日志索引的 `recent_days`（7）、`stale_days`、这里的 30 天是三个不同的数。

## Implementation Plan

- `crates/fibo-core/src/retention.rs`：规则、`leave_out`、`readopt`。
- `crates/fibo-host/src/host/archive.rs`：每小时一次、commit 前读回；`store/archive.rs`：按 CLI 会话和终端 id 读一行。
- 避免：在别处另写「这个会话还要不要」的判断；从 `agent_launch` / `session_names` 等内存表里删键。

## Verification

- `cargo test -p fibo-core --lib retention`
- `cargo test -p fibo-host --lib archive`（撤掉 `readopt_archived`，`an_archived_conversation_comes_back_as_itself` 报 UNIQUE 约束失败）

## Alternatives

见 spec：A 按时间删库会丢结局和名字；C 共享不可变数据不减数量，验收过不了。

## More Information

重开条件：归档行多到库本身变慢时，再按很长的期限（例如 180 天）加方案 A。
