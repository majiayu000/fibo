---
status: accepted
date: 2026-11-04
decision-makers: majiayu000
---

# 后台 shell：脚本自己连的端口不算服务，规则和真实案例记在这一处

## Context and Problem Statement

用户的原则是：**脚本在运行就算「在跑」，只有不会自己结束的东西才不算。** Claude 只告诉 Mooring 后台还有几个 shell，不说它会不会结束，所以 Mooring 只能看进程去猜。[按监听端口分任务和服务](2026-11-03-split-background-shells-by-listening-port.md)用「有没有固定端口」猜，这是间接线索。

2026-11-04 的 Crouton 测试会话判错了：`run2.sh` 先起两个中转（`opencode_relay.mjs`，监听 8898、8897），再让 Python 通过它们发请求，跑完一起退出。它有固定端口，于是被当成服务，显示「等后台」，排进了「要你处理」。这是同一类问题第二次出现，上一次是渲染脚本的临时端口（1ab8b0f）。

## Decision

下面的规则按顺序判断，第一条符合的算数。「任务」显示「在跑」；「服务」显示「等后台」，归在「要你处理」。

| # | 看到什么 | 算 | 为什么 |
|---|---|---|---|
| 1 | 已跑满 2 小时 | 服务 | 兜底：任何猜错的情况都不会永远转圈 |
| 2 | 同一棵进程树里，另一个进程连着树内监听的固定端口；看到一次就记住，只要树里还有不监听端口的末端进程 | 任务 | 服务是脚本的零件。连接时有时无，所以要记住；干活的进程都没了，就只剩服务 |
| 3 | 监听固定端口（< 49152），连续两次看到才换边 | 服务 | 它在等外面的人来访问 |
| 4 | 只有临时端口（≥ 49152），或没有端口 | 任务 | 临时端口是工具给自己用的，不是开给人访问的 |
| 5 | 在 Claude 的子进程里找不到这个 shell | 服务 | 看不到就不声称它在跑 |

不做：不按命令文字猜（名字叫 relay 的既可能是常驻服务，也可能是脚本的零件）；Codex 会话不覆盖（它的 hook 不报后台 shell）。

## 踩过的真实案例

以后再判错，就在这张表和 `background_shells.rs` 的 `known_cases` 各加一行，再改规则。规则改动不能让任何一行变红。

| 日期 | 情况 | 应该显示 | 靠哪条 | 来源 |
|---|---|---|---|---|
| 11-03 | 测试、构建、CI 轮询（Pulse 那类） | 在跑 | 4 | 3fb2791 |
| 11-03 | 开着的 `http.server`，或单独常驻的 relay | 等后台 | 3 | 3fb2791 |
| 11-03 | 测试里临时起一下服务 | 在跑 | 3 要连看两次 | 3fb2791 |
| 11-04 | 渲染脚本 `render.mjs` 给无头 Chrome 开临时端口 | 在跑 | 4 | 1ab8b0f |
| 11-04 | Crouton 脚本自带固定端口的中转和客户端 | 在跑 | 2 | 本次 |
| 11-04 | 同一脚本，第一次看时客户端还没连上 | 先短暂等后台，连上后在跑 | 2 | 本次 |
| 11-04 | Crouton 的 `run5.sh`：客户端跑完，`wait` 卡在中转上，脚本退不出 | 等后台 | 2 不再成立，落到 3 | 本次 |
| — | `tail -f`、不监听的监视程序 | 2 小时前在跑，之后等后台 | 4、1 | 3fb2791 |

## Consequences

* Good, because 判断更接近真正的问题（它会不会自己结束），也不用猜命令文字。
* Good, because 每次判错都留下一个案例和测试，规则只会越积越准。
* Bad, because 客户端连上之前可能显示几秒「等后台」。
* Bad, because 脚本卡在 `wait` 又留着别的不监听端口的末端进程时，仍显示「在跑」，最多到 2 小时。
* Bad, because 树内互相连接的常驻服务（比如 `concurrently` 起的前端代理到自己的 API）会算任务，要到 2 小时才变成服务。真遇到时加案例再改。

## Implementation Plan

* **Affected paths**: `crates/mooring-host/src/background_shells.rs`（`judge`、`Sockets`、`known_cases`），状态归类仍只在 `crates/mooring-core/src/status.rs`。
* **Patterns to follow**: 探测结果只叠加在快照上（`Background.jobs`），状态只由 `status::of` 判定；每次规则改动都先加案例。
* **Patterns to avoid**: 不要退回只数 shell 个数；不要按命令文字判断；不要用「shell 外面包了一层脚本」判断，`npm run dev` 也是这样。

### Verification

- [ ] `cargo test -p mooring-host --lib background_shells`：`known_cases` 是案例表，`a_real_script_using_its_own_port_differs_from_a_server_with_an_outside_client` 用真实进程验证。

## Alternatives

* 只要 shell 活着就算在跑：否定，`http.server` 会一直转圈，11-03 已经吃过亏。
* 外层是脚本就算任务：否定，`npm run dev` 这类服务也包着一层。

## More Information

* 用户原话（11-04）：「脚本在运行这件事情……我记得好像是要放到在跑的里面呢」；11-03：「这个在运行脚本的不是再跑吗」；对记录的要求：「我们以后可能就有更多的规则，是不是慢慢的会完善起来」。
* 重开条件：常驻服务被第 2 条误判成任务的案例出现，并且不能靠收窄第 2 条解决。
