---
status: accepted
date: 2026-11-03
decision-makers: majiayu000
---

# 后台 shell 按有没有监听端口分成任务和服务

## Context and Problem Statement

回合结束后留下的后台 shell 有两类：会结束的（测试、构建、下载、轮询），Claude 会在它退出时被唤醒接着干；不会结束的（`http.server`、relay、`tail -f`）。只数个数的两种做法都错过：一律算在跑，服务永远转圈；一律算等后台（[一律算等后台](2026-11-03-count-background-shells-as-waiting.md)），测试被误报成要你处理。

## Decision

逐个看 shell 的进程树：

* 监听固定 TCP 端口（< 49152），或已跑超过 2 小时：算**服务**，会话显示「等后台」。
* 其余算**任务**，会话显示「在跑」，第二行写「等后台任务 <描述>」。
* 换边要连续两次观察（每 5 秒一次）都不一致，所以测试里临时起一下服务仍算任务。

不做：不按命令文字猜；Codex 会话不覆盖（Codex 的 hook 不报后台 shell）。

## Consequences

* Good, because 真实会话（relay、`http.server`、CI 轮询对比）实测全部判对。
* Bad, because 每 5 秒跑一次 `lsof`；不监听端口又会一直跑的东西要等满 2 小时才算服务。

## Implementation Plan

* **Affected paths**: `crates/fibo-host/src/background_shells.rs`（`judge`、`look`、`JOB_LIMIT`、`EPHEMERAL`），`crates/fibo-host/src/host/shell_watch.rs`。
* **Dependencies**: Stop hook 的 `background_tasks` 带每个 shell 的 description 和 command（Claude Code 2.1.317）。
* **Patterns to follow**: 命令只放在后台内存里用来对上子进程，不写进存储；结果只叠加在快照上（`Background.jobs`、`ShellTask.job`）。
* **Patterns to avoid**: 不要回到只数 shell 个数；不要按命令文字判断。

### Verification

- [ ] `cargo test -p fibo-host background_shells` 通过（5 个用例，含 `only_a_chosen_port_makes_a_server`）

## More Information

* 3fb2791（2026-11-03）实现。
* 2026-11-04 补：临时端口不算服务（1ab8b0f）。渲染脚本 `render.mjs` 绑 0 拿临时端口给无头 Chrome 用，曾把正在渲染的会话误判成等后台。
* 2026-11-04 补：脚本自己连的固定端口也不算服务，现行规则和案例表见[这条记录](2026-11-04-background-shell-own-client.md)。
