---
status: "superseded by [按监听端口分](2026-11-03-split-background-shells-by-listening-port.md)"
date: 2026-11-03
decision-makers: majiayu000
---

# 回合结束后只剩后台 shell 的会话一律算「等后台」

## Context and Problem Statement

Claude 回合结束后，还可能留着后台 shell。最早一律算「在跑」，开着的 `http.server`、relay 会让侧栏一直转圈，第二行挂着回合里最后一条命令。

## Decision

1058984（2026-11-03）：只剩后台 shell 或定时唤醒的会话，并入「要你处理」画绿圈，第二行写「后台仍有 shell N 个」；看板进「等你验收」列，插件收到 done。

## Consequences

* Good, because 开着服务的会话不再一直转圈。
* Bad, because 回合结束后还在跑的测试、构建、CI 轮询也被当成做完了，被误报成要你处理（Pulse 那类测试）。

## Implementation Plan

* **Affected paths**: `crates/fibo-core` 的状态判定（`status::of`）。

### Verification

- [ ] 已被取代，不再验证。

## Alternatives Considered

* 一律算在跑（之前的做法）：服务永远转圈。

## More Information

2026-11-03 当天被 [按监听端口分](2026-11-03-split-background-shells-by-listening-port.md) 取代。教训：只数 shell 个数，两种情况总有一种判错。
