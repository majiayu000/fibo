---
status: accepted
date: 2026-11-26
decision-makers: majiayu000
---

# 国内外两台中继：电脑两边都挂，手机两边一起拨、先通的留下

## Context

官方中继准备对外开放，用户可能在国内，也可能在国外。用户：「我可能会找一个国内的节点，或者找一个国外的节点……就是双节点，然后可能会根据用户的实际的网速来选节点」。现有一台上海节点，11-25 新加一台香港节点。

中继只在内存里把同一个路由码的电脑和手机接起来，中继之间互不知道。所以只让手机自己测速选节点是不行的：电脑在上海节点、手机选了香港节点，就碰不上。

## Decision

- **电脑同时挂在每一台中继上。** `remote-relay.json` 在 `relay` / `key` 之外加 `also: [{relay, key}]`；`fibo remote` 这个子进程在同一个运行时里对每台中继各跑一条 `stay_connected`。路由、撤销、会话都按中继各管各的；一台拒绝（密钥不对、版本太旧）只停那一台，其余照常。`Live` 按中继记状态，任意一台连上就算「已连上」。
- **手机拨号时每台中继都拨，先握完手的留下，其余关掉。** 握手要经过 手机→中继→电脑 整条路，先完成的就是这一刻最快的路，不另外测速、不花额外流量。每次重连都重新比，不做粘滞。全都失败时，报主中继（`relay`）的错误，和只有一台时一样。
- **手机怎么知道有哪些中继：** 配对链接多带 `alt=<中继>`（可重复），已经配过的手机从 `hello` 的 `relays` 里学到，改写存着的链接。列表来自已经配对、握过手的电脑，不另设配置接口，也就不用给它签名。
- **不做：** GeoDNS / Anycast（只看手机这一头）；中继之间互相转发（流量走两遍，跨境那段一样不稳）；P2P 打洞（国内移动网络大多在运营商 NAT 后面）；按带宽测速（花流量）。

## Consequences

- 每台电脑多一条空闲连接，只走心跳：中继每 25 秒、电脑每 30 秒 ping 一次，估算每月 10–20 MB，没有实测。手机在用时的流量只走被选中的那一台。
- 每次拨号时，电脑上会多开一个握完手马上被关掉的会话，日志里会多一对 `connected` / `left`。
- 兼容：
  - 旧手机认不出 `alt=`，按字段名取值时直接跳过，只用主中继。
  - 旧电脑不发 `relays`，新手机就只拨链接里的那几台。
  - 旧的 `remote-relay.json` 没有 `also`，读出来是空列表，和现在一样只挂一台。
  - 旧版本程序读到带 `also` 的文件时不认识这个字段（serde 不拒绝未知字段），也只挂主中继。
  - 主中继的地址一直不变，所以已经配对的手机在学到第二台之前照常能用。
- 中继地址不进公开仓库（[2026-11-08](2026-11-08-scrub-relay-address.md)）：本文件和提交里只写 `<国内中继>`、`<香港中继>`。

## Implementation Plan

- `crates/fibo-remote/src/link.rs`、`mobile/src/remote/link.ts`：`alts`，`alt=` 可重复。
- `crates/fibo-host/src/host/remote.rs`：`Config.also`；子进程通过环境变量 `MOORING_RELAY_ALSO` 拿到额外的中继和它们的密钥（JSON），和 `MOORING_RELAY_KEY` 一样不进命令行。
- `crates/fibo-host/src/relay.rs`、`relay/live.rs`：每台中继一条 `stay_connected`，`Live::scoped`。
- `crates/fibo-host/src/remote.rs`：`hello` 带 `relays`，从 `remote_config` 读（只有一个来源）。
- `mobile/src/remote/client.ts`：`Remote.connectAny`；`hub.ts` 的 `dial` 改用它，`hello.relays` 有新的就存下来。
- 命令行：`fibo remote --enable --relay <国内中继> --relay-key <密钥> --also <香港中继> --also-key <密钥>`。设置页暂不加第二台中继的输入框。

## Verification

```sh
cargo test -p fibo-remote link
cargo test -p fibo-host relay
cd mobile && bun test src/remote
```

实测：电脑挂两台，手机拨号时各种情况都要连得上——两台都通、关掉国内那台、关掉香港那台。看 `fibo remote --status` 里两台的状态，以及手机连上的是哪一台。

## Alternatives

- 每台电脑只连一个「主节点」（Tailscale DERP 的做法），再由电脑告诉手机去哪台：最省连接，但主节点挂掉时电脑要先换节点、手机拿着旧地址要试一圈，这段时间连不上。电脑数量到几万台、空闲连接真的占资源时再考虑。

## More Information

- 重新考虑的条件：空闲连接的心跳流量超过中继总流量的 5%，或者拨号时多出来的那个会话造成了可见的问题。
- 来源：本会话 11-26 用户选定「方案一（同时连两个）」。
