---
status: accepted
date: 2026-11-07
decision-makers: majiayu000
---

# 服务不算在等：只留一个服务的回合是等你验收

## Context and Problem Statement

用户看到 Codex 显示「等后台 · 1时」，问「到底在等什么」。实际是它留了一个预览用的 `http.server`，没人连，也不会结束。「等后台」同时装着两件事：监视任务和定时唤醒（Agent 真的会被叫醒）和常驻服务（什么都不会回来）。用户追问「是不是真的有在等这个东西」，答案对服务是否定的。

## Decision

| 回合结束后还留着 | 状态 | 小字 |
|---|---|---|
| 子 Agent、任务（测试、构建） | 在跑 | 正在等后台任务 … |
| 监视任务、定时唤醒 | 等后台 | 后台仍有监视任务 1 个 |
| 常驻服务 | **等你验收** | 后台留着 1 个服务 · 端口 5569 |
| 没有端口或还没看过的进程（`tail -f`、超 2 小时的） | **等你验收** | 后台留着 1 个进程 |

任务和服务怎么分不变，仍按[这条记录](2026-11-04-background-shell-own-client.md)。小字只有一份，在 `Content::left_behind`；侧栏、窗格、状态说明和手机都读它。

不做：不加新状态，一个回合结束了就是等你验收，小字只是备注。

**逐个验收仍然不收留着服务的会话。** 它的回车会关终端，以前靠「等后台」不等于「等你验收」挡住，现在要明说（`reviewable(status, left_behind)`）。

## Consequences

* Good, because 「等后台」出现时就真有东西会把 Agent 叫醒。
* Good, because 留着服务的回合也有回合条（看改动、标记完成），它本来就结束了。
* Bad, because 「等你验收」的卡片里有一些进不了逐个验收，只能打开或跳过。页面上写明原因。
* Bad, because 待办「完成」时关终端这条路没变：留着服务的终端和以前一样会被关。

## Implementation Plan

* **Affected paths**: `crates/mooring-core/src/status.rs`（`live`、`recorded`、`left_behind`），`crates/mooring-core/src/content.rs`（`Content::left_behind`、`ShellTask.port/ended`），`crates/mooring-host/src/background_shells.rs`（`Seen.port`），`crates/mooring-host/src/remote/list.rs`（手机的第三行），`crates/mooring-app/src/workbench/review_queue.rs`。
* **Patterns to follow**: 端口和 `ended` 同 `job`，只叠加在活快照上。
* **Patterns to avoid**: 不要让 `reviewable` 只看状态；不要把进程没看过的情况写成「服务」。

### Verification

- [ ] `cargo test -p mooring-host --test managed_session a_server_codex`：真 `http.server`，等你验收，小字带端口。
- [ ] `cargo test -p mooring-core a_finished_turn_names`：小字的各种情况。
- [ ] `cargo test -p mooring-app review_queue`：留着服务时不可标记。

## Alternatives

* 把词改成「留着服务」：否定，多一个状态，而回合确实已经结束，用户要做的和等你验收一样。

## More Information

* 用户原话（11-07）：「这里为什么是说在等候台呢？到底在等什么呢？我看不懂啊」「等后台这件事情是不是真的有在等这个东西呢」。
* 重开条件：用户因为留着服务的会话进了等你验收而漏看服务，或嫌它们进不了逐个验收。
