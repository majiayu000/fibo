---
status: accepted
date: 2026-11-27
decision-makers: majiayu000
---

# 电脑经中继连另一台电脑：中继会话直接接原生协议，只给完全控制

## Context

一台电脑的窗口连另一台，原来只有 `ssh -L`（[11-14](2026-11-14-one-window-many-hosts.md)）。没装 Tailscale、ssh 进不去的用户就用不了；中继（`fibo-relay`）只接手机。用户原话：「我一定要做的是电脑之间的中继，而不是只做手机」「可以自动选路，可以让用户选择 tailscale 和服务器中继，应该是都支持的」。

同类做法（Paseo、t3code、VS Code 隧道、Coder、Pane）都是一台电脑挂多条路，中继只是其中一条。实测两台 Mac 之间，Tailscale 上的 ssh 往返中位约 17 ms，经国内中继估算 82～94 ms：中继解决「连得上」，不解决快。

## Decision

- 配对记录（`remote-devices.json`）多一个 `kind`：`phone`（缺省，旧文件都读成它）或 `computer`。会话接手机协议还是原生协议**只看主机侧这条记录**，不看对方连接时说什么。
- `computer` 的会话只在 `role == full` 时接到本机 `workbench.sock`，和 `ssh -L` 给的完全一样；改成别的权限的电脑被拒（`close::UNPAIRED`）并写进 `remote-log.jsonl`，不会退回手机协议。手机永远拿不到原生协议。
- 会话开始先发一行 `About`（home、shell、机器 id 和名字），替代 ssh 时 `remote_socket` 那段脚本。
- 发起方：`fibo remote join '<链接>'` 把链接存到 `<config>/relay-links/<名字>.link`（0600），`machines.json` 那台写 `"via": "relay"`；窗口照 `ssh -L` 的样子起 `fibo remote dial --listen <m/<host>/workbench.sock>`，每条本地连接一条中继会话（做法 A）。`hosts.rs`、`attach.rs` 之外的显示代码不知道底下是哪条路。
- `About` 之后两个方向都是一条 zstd 流（`relay/squeeze.rs`，窗口 8 MB，每块都 flush），和 ssh 的 `Compression=yes` 一个作用；两边的窗口按压缩后的字节算。
- `dial --listen` 随启动它的窗口退出（`exit_with_parent`）：窗口崩了或被 TERM 时，留下的进程会一直占着中继和那台的会话。
- 机器标题的测延迟改成复用一条连接（原来每 5 秒新开一条）：走中继时每条新连接都是中继的一条新会话，占每个地址每分钟 120 条的名额。

不做（这一步）：自动选路和手动切换（下一步，已在 [routes.md](../multi-machine/routes.md) 做了：用单独的 `auto` 字段，没有用 `via: "auto"`，因为旧版读到不认识的 `via` 会整份 machines.json 报错）、窗口里的配对界面、同网直连、打洞、原生协议按操作分权限。

## Consequences

- 原生协议本身不按操作检查权限，经中继连过来的电脑能做 ssh 进来能做的一切；配对时 CLI 明说「它就是这台电脑的钥匙」。撤销和踢下线沿用手机那套，一秒内断开。
- 经中继的电脑上 `Host::run`（在那台跑 `fibo` 命令，如 hook 状态）不可用，返回说明而不是悄悄失败。
- 主机往中继发的队列改成不限长：会话在通道锁里封好就入队以保证顺序，队列满时它在锁里等，而唯一清队列的循环正等这把锁去解一个 ACK，一个 2 MB 的工作区就把整台机器从中继上卡掉（实机碰到的，`computer_tests` 守着）。长度由每个会话自己的窗口（1 MB 未确认）限住。手机也走这条路，只是它的量小，以前没碰上。
- 每条本地连接一次中继建连（TCP + TLS + WebSocket + Noise）。窗格多、请求多时比 ssh 慢得多；主机每台最多 64 条会话（`MAX_SESSIONS`）。超过时改成一条会话里多路复用（做法 B）。

## Implementation Plan

- 主机：`crates/fibo-host/src/devices.rs`（`Kind`、`pair_computer_link`）、`relay/connect.rs`（按 `kind` 分流、拒绝非完全控制）、`relay/bridge.rs`（`Serve::Native`、`say`）、`relay/computer.rs`（`About`、`join`）。
- 发起方：`relay/dial.rs`；窗口 `crates/fibo-app/src/machines.rs`（`Via`）、`attach.rs`（`relay_about`、`forward_argv`）、`workbench/hosts.rs`（测延迟复用连接）。
- 不要让客户端在握手或第一条消息里选协议；不要给 `computer` 退回手机协议。

## Verification

- `cargo test -p fibo-host --bin fibo computer`：电脑接到原生 socket、降成只看被拒、手机仍是手机协议、日志写了 `kind`。
- 实机：被连的电脑 `fibo remote pair --relay <中继> --computer --name <名字>`，另一台 `fibo remote join '<链接>' --name <名字>`，关掉 ssh 也能在侧栏看到并操作那台。

实测（11-27，mac1 经国内中继连 mac2，同一时段对照 Tailscale 上的 ssh，各 100 次）：

| | ssh | 中继 |
|---|---|---|
| 已开连接上一个来回，中位 / p90 | 16 ms / 32 ms | 139 ms / 157 ms |
| 新开一条连接到 Hello | 0.03～0.05 s | 约 0.5 s |
| 整份工作区 2.25 MB | 0.11～0.33 s | 0.40～0.43 s（压缩前 2.6～3.4 s） |

所以中继只该在 ssh 连不上时用：下一步是一台机器挂两条路、自动选快的。

## Alternatives

- 窗口在中继上说手机协议（它自带角色和记录）：11-15 已否，侧栏和主区域会对不上。
- 客户端握手时声明「我是电脑」：被冒充的面太大，完全控制的手机密钥就能拿到原生协议。
- 一条 Noise 会话里多路复用（做法 B）：开窗格不再建连，但要新写，先量做法 A 够不够。

## More Information

同类调研没有放进仓库。重开条件：新开窗格到出画面超过 1 秒、常开超过 50 个窗格，或每分钟新开中继会话接近 100 条。
