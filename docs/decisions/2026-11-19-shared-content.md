---
status: accepted
date: 2026-11-19
decision-makers: maintainer
---

# 会话内容在工作区里共享，改的时候才复制

## 背景

接 [2026-11-19 后台刷新只复制会变的](2026-11-19-host-refresh-cost.md)。11-19 在真实安装上采样（App 0b7deb76，存 5088 条会话、发 1540 条，71 个终端）：后台约 17–25% 个核，忙的时间里 39% 在复制工作区（`Workspace::clone`），其中会话内容表（`content`）占 29%；日志关联每批先复制整份工作区（`Association::apply`），又占 15%。窗口那边把收到的差异拼回整份工作区（`Held::take`），占窗口忙时间的 23%，其中大半也是复制内容表。

复制 → 改 → 提交是后台所有写入的通用做法，上一条决定没动它，因为要改每一处写入路径。

## 决定

`Workspace::content` 的值从 `Content` 改成 `Arc<Content>`。复制工作区只复制指针；要改某条内容时用 `Arc::make_mut`，只有那一条被复制（写时复制）。写入路径照旧是复制 → 改 → 提交，不用逐处记账，漏写一处也不会显示旧状态，只是那一处多复制一份。

内容哈希的缓存先比指针（`Arc::ptr_eq`），指针相同就是没改过，不再逐字段比较。

线上格式不变：serde 的 `rc` 特性让 `Arc<Content>` 序列化成和原来逐字节相同的 JSON，旧窗口、手机、另一台电脑和旧数据都照旧。

不做：会话列表（`agent_sessions`）还是 `Vec<AgentSession>`，见下面「没做的」。

## 结果

测试工作区（`perf-fixture.py prepare … 870 60`，`SUB=671 OLD=3500`，共 1541 条发、5041 条存），`perf-refresh.py … 40`，两轮交替：

- 后台每次刷新：62.1 / 62.5 M → 48.5 / 49.0 M 条指令（-22%）
- 窗口端（临时基准，同一份 1541 条工作区，改一条内容）：拼回工作区 0.95–1.6 ms → 0.51 ms，窗口那次整份复制 0.83–1.03 ms → 0.28 ms（墙钟时间，机器负载高）

## 没做的

- 会话列表也共享（`Vec<Arc<AgentSession>>`）：改完内容后再采样，剩下的大头是会话列表的整份复制、提交前逐条比较（`Store::save_applying`）和整份丢弃，估计还能再省约 25–30%。但只核心就有 35 处编译错误，窗口端更多，而且和同时进行的窗口收增量优化（`workspace_delta.rs`）直接冲突，先不做。重开条件：那条线合入以后，会话数再翻倍，或者采样里会话复制重新成为第一。
- 哈希改成先写进缓冲再一次性用 foldhash：48.8/49.1 → 48.3/48.6 M，约 1%，低于 10% 的放弃线。剩下的 SipHash 来自标准库 HashMap 的键，不在差异哈希里。

## 验证

`sh ci-test.sh`；`cargo clippy --workspace --all-targets -- -D warnings`（另加 `--features fibo-app/visual-test`）；`python3 scripts/perf-refresh.py RUN_DIR BIN 40` 对照改前改后的 `fibo`。
