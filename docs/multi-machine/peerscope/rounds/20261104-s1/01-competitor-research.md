# 连接方式：同类产品怎么让两台设备连上（2026-11-04，源码版）

图例（证据等级）：[E3] 读过固定版本的源码；[E2] 只看了官方文档或 README；[D1] 只有零星讨论。第一版只读了文档（全是 [E2]），这一版把开源的 8 家都换成了源码依据，闭源的 Rex、VS Code、Claude Remote Control 仍是文档。

## 证据边界

| 来源 | 能支持 | 不能支持 |
|---|---|---|
| 8 个开源仓库的固定 commit 源码（下表） | 默认服务器、连接顺序、中继能看到什么、设置项名字 | 实际延迟、打洞成功率；没编译、没运行、没抓包 |
| Rex 官方文档 + 第三方开源兼容服务端 tuxosaurus 的源码 | Rex 的连法、准入规则、协议形状 | Rex 本身的实现（闭源） |
| VS Code、Claude Remote Control 官方文档 | 连法和加密说法 | 实现 |

| 仓库 | 固定版本 |
|---|---|
| rustdesk / hbb_common / rustdesk-server | `55786aaa` / `fae47e02` / `2215e701` |
| tailscale | `4290c03d` |
| syncthing | `b3473437` |
| iroh | `f3503949` |
| happy | `c2f3801a` |
| paseo | `67628b48` |
| upterm | `87f8ee86` |
| sshx | `3604e8ec` |
| tuxosaurus（Rex 的 Linux 兼容服务端） | `0c12586d` |

## 候选池（16 个）

| 产品 | 做什么 | 采用度 | 连法（一句话） |
|---|---|---|---|
| [Rex](https://www.superlogical.com/rex)（Superlogical，Ghostty 作者的公司） | 给 AI Agent 用的常驻会话终端，Mac/iOS/Linux | 闭源，公测免费 | 没有自己的中继：内置 Tailscale 节点，或 Tailscale Serve、tailnet 地址、自带 VPN |
| [RustDesk](https://github.com/rustdesk/rustdesk) | 远程桌面 | 128,104★ | 官方服务器默认；先打洞，不通才中继 |
| [Tailscale](https://github.com/tailscale/tailscale) | 组网 | — | 直连优先，DERP 兜底，持续找更好的路 |
| [Syncthing](https://github.com/syncthing/syncthing) | 文件同步 | 90,053★ | 公共中继池默认开；有更好的连法就换掉中继 |
| [iroh](https://github.com/n0-computer/iroh) | Rust P2P 库 | 12,921★ | 4 个官方区域中继默认；打洞后在同一连接里切到直连 |
| [Happy](https://github.com/slopus/happy) | 手机遥控 Claude Code | 24,464★ | 只走服务器（官方默认，可自建），存加密数据 |
| [Paseo](https://github.com/getpaseo/paseo) | 多端遥控 Agent | 22,879★ | 直连（TCP/ssh）为主；官方中继新装默认关 |
| [upterm](https://github.com/owenthereal/upterm) | 终端共享 | 1,332★ | 只走服务器（官方默认，可自建），服务器能读内容 |
| [sshx](https://github.com/ekzhang/sshx) | 网页终端共享 | 7,711★ | 只走服务器（官方默认，代码里可自建），端到端加密 |
| [VS Code 隧道](https://code.visualstudio.com/docs/remote/tunnels) | 远程开发 | — | 只走微软服务，要登录账号 |
| [Claude Remote Control](https://code.claude.com/docs/en/remote-control) | 手机接本机 Claude | — | 只走 Anthropic，文档只写了 TLS |
| [Omnara](https://github.com/omnara-ai/omnara) | 托管遥控平台 | 3,349★ | 走平台（11-03 那轮：自述没有端到端） |
| [tmate](https://github.com/tmate-io/tmate) | 终端共享 | 6,135★ | 官方服务器，可自建（未细看） |
| [ZeroTier](https://github.com/zerotier/ZeroTierOne) / [frp](https://github.com/fatedier/frp) | 组网 / 穿透 | 17,232★ / 110,434★ | 未细看 |
| ssh + Tailscale/局域网 | 替代物 | — | Mooring 现在桌面连桌面的方式 |

## 最相关的一家：Rex

Rex 和 Mooring 几乎同一个定位：后台持有会话，窗口只是客户端，关掉窗口会话还在。它的 Linux 兼容服务端 tuxosaurus 也用 libghostty-vt 给客户端出整屏快照，和 Mooring 一样。

- **没有自己的中继**。官方文档列的远程方式只有四种：内置 Tailscale 节点（`rex server start --tailscale`，地址是 `https://rex.<tailnet>.ts.net`）、Tailscale Serve、tailnet 地址（`--extra-listen tailnet://:7297`），以及明文 HTTP（`--allow-anonymous`，文档警告「Every connection then has full access」）。另外说可以配「their own corporate VPN, ZeroTier」（[文档](https://www.superlogical.com/rex/docs/linux/remote-access.md)）[E2]。
- **内置 Tailscale，而不是要求装 Tailscale**：服务端用 tsnet 把自己作为一台独立设备加入 tailnet，在 443 上用 Tailscale 证书开 HTTPS（[node.go#L115](https://github.com/wesleygrimes/tuxosaurus/blob/0c12586d54adc562e57339da5ac2bfdaf9735db7/internal/tailnet/node.go#L115)、[#L309](https://github.com/wesleygrimes/tuxosaurus/blob/0c12586d54adc562e57339da5ac2bfdaf9735db7/internal/tailnet/node.go#L309)）[E3，兼容实现]。
- **身份就是 Tailscale 账号，没有配对码**：每个连接用 WhoIs 查出对方的 Tailscale 账号，默认只放这台服务器的主人，带 tag 的设备和别的账号一律拒绝（[node.go#L429-L436](https://github.com/wesleygrimes/tuxosaurus/blob/0c12586d54adc562e57339da5ac2bfdaf9735db7/internal/tailnet/node.go#L429)）；`--tailscale-allow-tailnet-members` 放宽 [E3，兼容实现]。开关 Tailscale 只能在本机 Unix socket 上做，远程连接做不了 [E2]。
- **断线续传，不重发整屏**：终端输出走单独的数据流，凭一次性 ticket 连上；断线后带着「已收到多少字节」重连，服务器在 8 MiB、60 秒内补发缺的部分，超出才重新要整屏（[protocol.md](https://github.com/wesleygrimes/tuxosaurus/blob/0c12586d54adc562e57339da5ac2bfdaf9735db7/docs/protocol.md)）[E3，兼容实现]。
- 实测（第三方文章）：经 tailnet 一条 CLI 调用 0.07–0.09 秒；开 HTTPS 证书会把设备名写进公开的证书透明日志（[classmethod](https://dev.classmethod.jp/en/articles/reona-superlogical-rex-beta-try/)）[E2]。

## 各家中继的内部构造

| 中继 | 状态 | 怎么找到对方 | 能看到什么 | 证据 |
|---|---|---|---|---|
| **Happy 服务器** | 有状态：数据库存会话和消息，socket.io 推送 | 账号 | 内容加密（每会话一把 AES-256-GCM 密钥，用 box 包给账号公钥）；会话 id、tag、seq、在线状态、时间戳是明文 | [schema.prisma#L95-L114](https://github.com/slopus/happy/blob/c2f3801a9d64ff93249c0d262909f052f1201381/packages/happy-server/prisma/schema.prisma#L95)、[api.ts#L43-L52](https://github.com/slopus/happy/blob/c2f3801a9d64ff93249c0d262909f052f1201381/packages/happy-cli/src/api/api.ts#L43)、[encryption.ts#L154](https://github.com/slopus/happy/blob/c2f3801a9d64ff93249c0d262909f052f1201381/packages/happy-cli/src/api/encryption.ts#L154) [E3] |
| **Paseo 中继** | 无状态：Cloudflare Worker 转发 | 配对链接 | Noise_NK_25519_ChaChaPoly_SHA256，绑定 daemon 静态公钥；公钥放在链接 `#` 后面，不经过服务器 | [wrangler.toml](https://github.com/getpaseo/paseo/blob/67628b48fe4c6ec05fbaccca7be4573605c5c5b8/packages/relay/wrangler.toml#L1)、[secure-channel.ts#L10-L18](https://github.com/getpaseo/paseo/blob/67628b48fe4c6ec05fbaccca7be4573605c5c5b8/packages/relay/src/secure-channel.ts#L10)、[pairing-offer.ts#L42-L51](https://github.com/getpaseo/paseo/blob/67628b48fe4c6ec05fbaccca7be4573605c5c5b8/packages/server/src/server/pairing-offer.ts#L42) [E3] |
| **RustDesk hbbs + hbbr** | hbbs 管 ID、帮打洞、给公钥签名；hbbr 只转字节 | 设备 ID | hbbr 看到 uuid、IP、密文。会话密钥用 box 封装，信任根是服务器公钥 `key`；**验签失败会退回明文** | [relay_server.rs#L537](https://github.com/rustdesk/rustdesk-server/blob/2215e7014539a3172ad0d3015b5e5b1f685469b7/src/relay_server.rs#L537)、[client.rs#L1689-L1700](https://github.com/rustdesk/rustdesk/blob/55786aaa5709ac0a450aabcf06f9d1fee3119908/src/client.rs#L1689) [E3] |
| **Syncthing strelaysrv** | 无状态拼接 | 设备 ID（加入时从证书得到） | 双方 IP、谁找谁、总字节数（还会公开在状态页）；两端在中继上自己做 TLS | [relay_dial.go#L52-L57](https://github.com/syncthing/syncthing/blob/b34734376128492fa4e1b01412193de033e20613/lib/connections/relay_dial.go#L52)、[session.go#L247-L268](https://github.com/syncthing/syncthing/blob/b34734376128492fa4e1b01412193de033e20613/cmd/strelaysrv/session.go#L247) [E3] |
| **Tailscale DERP** | 无状态，按目标公钥转包 | 节点公钥 | 帧只有「32 字节目标公钥 + 包」，包是 WireGuard 密文 | [derp.go#L75-L77](https://github.com/tailscale/tailscale/blob/4290c03d37d0956e6ab0421bbe85fdc764ff1c50/derp/derp.go#L75)、[endpoint.go#L1196](https://github.com/tailscale/tailscale/blob/4290c03d37d0956e6ab0421bbe85fdc764ff1c50/wgengine/magicsock/endpoint.go#L1196) [E3] |
| **iroh-relay** | 无状态，按 EndpointId 转发 | EndpointId（Ed25519 公钥） | 对端 IP、EndpointId、字节数；TLS 校验对端公钥必须等于目标 EndpointId | [verifier.rs#L33-L75](https://github.com/n0-computer/iroh/blob/f35039493803c240595cc7b56a976b8bb14f8f05/iroh/src/tls/verifier.rs#L33)、[http_server.rs#L491](https://github.com/n0-computer/iroh/blob/f35039493803c240595cc7b56a976b8bb14f8f05/iroh-relay/src/server/http_server.rs#L491) [E3] |
| **upterm uptermd** | 做 SSH 中间人：先当服务端接访客，再当客户端连主机 | 会话名 | **能读到内容** | [sshstock.go#L281](https://github.com/owenthereal/upterm/blob/87f8ee86fb34a7a9ca8e0f0e9f34a0189758a0da/server/sshstock.go#L281)、[#L395](https://github.com/owenthereal/upterm/blob/87f8ee86fb34a7a9ca8e0f0e9f34a0189758a0da/server/sshstock.go#L395) [E3] |
| **sshx 服务器** | 有状态，转发加密的终端流 | 会话链接 | 密钥只在链接 `#` 后面；服务器只收到加密后的零块和口令哈希；AES-128-CTR，Argon2id 的盐是固定字符串 | [controller.rs#L83-L90](https://github.com/ekzhang/sshx/blob/3604e8ec5ce0e741bd4a7de2e339b980ca8a2d40/crates/sshx/src/controller.rs#L83)、[encrypt.rs#L5-L31](https://github.com/ekzhang/sshx/blob/3604e8ec5ce0e741bd4a7de2e339b980ca8a2d40/crates/sshx/src/encrypt.rs#L5) [E3] |
| **Mooring fibo-relay**（对照） | 无状态 WebSocket 转发 | 每台手机自己的路由码，中继只存 sha256 | Noise 端到端；能看到两端 IP、路由码哈希、时间和字节数 | [server.rs](https://github.com/majiayu000/mooring/blob/d1c8e70d2f32b027512967318e6e7debcfe05ab2/crates/fibo-relay/src/server.rs) |

## 默认值和设置项（源码）

| 产品 | 默认服务器 | 用户在哪改 | 证据 |
|---|---|---|---|
| RustDesk | 写死 `rs-ny.rustdesk.com` 和内置公钥 `RS_PUB_KEY`；什么都不设就用它 | `custom-rendezvous-server`、`relay-server`、`key`（界面「⋮ → 网络」） | [config.rs#L116-L120](https://github.com/rustdesk/hbb_common/blob/fae47e024dd83cef6aaecde0aa7d11731df11649/src/config.rs#L116)、[#L1096](https://github.com/rustdesk/hbb_common/blob/fae47e024dd83cef6aaecde0aa7d11731df11649/src/config.rs#L1096) [E3] |
| Happy | 手机和 CLI 都默认 `https://api.cluster-fluster.com` | 手机服务器设置页；CLI `HAPPY_SERVER_URL` 或 settings 的 `serverUrl` | [serverConfig.ts#L9-L27](https://github.com/slopus/happy/blob/c2f3801a9d64ff93249c0d262909f052f1201381/packages/happy-app/sources/sync/serverConfig.ts#L9)、[configuration.ts#L57-L60](https://github.com/slopus/happy/blob/c2f3801a9d64ff93249c0d262909f052f1201381/packages/happy-cli/src/configuration.ts#L57) [E3] |
| Paseo | 新装 `relay.enabled: false`，只听 `127.0.0.1:6767`；开了才用官方 `relay.paseo.sh:443`（旧配置到 2027-01-31 仍视为开） | `--relay` / `--no-relay`，`PASEO_RELAY_ENDPOINT` | [persisted-config.ts#L382-L391](https://github.com/getpaseo/paseo/blob/67628b48fe4c6ec05fbaccca7be4573605c5c5b8/packages/server/src/server/persisted-config.ts#L382)、[config.ts#L39](https://github.com/getpaseo/paseo/blob/67628b48fe4c6ec05fbaccca7be4573605c5c5b8/packages/server/src/server/config.ts#L39) [E3] |
| Syncthing | `default` 展开为 tcp、quic 和 `dynamic+https://relays.syncthing.net/endpoint` | 监听地址、`relaysEnabled` | [config.go#L49-L53](https://github.com/syncthing/syncthing/blob/b34734376128492fa4e1b01412193de033e20613/lib/config/config.go#L49) [E3] |
| iroh | 4 个区域的 `*.relay.n0.iroh.link` | `RelayMode::Default / Staging / Custom / Disabled` | [defaults.rs#L27-L33](https://github.com/n0-computer/iroh/blob/f35039493803c240595cc7b56a976b8bb14f8f05/iroh/src/defaults.rs#L27)、[endpoint.rs#L1999](https://github.com/n0-computer/iroh/blob/f35039493803c240595cc7b56a976b8bb14f8f05/iroh/src/endpoint.rs#L1999) [E3] |
| upterm | `ssh://uptermd.upterm.dev:22` | `--server` | [host.go#L253](https://github.com/owenthereal/upterm/blob/87f8ee86fb34a7a9ca8e0f0e9f34a0189758a0da/cmd/upterm/command/host.go#L253) [E3] |
| sshx | `https://sshx.io` | `--server`、`SSHX_SERVER`（README 说不支持自建，代码里有） | [main.rs#L15-L16](https://github.com/ekzhang/sshx/blob/3604e8ec5ce0e741bd4a7de2e339b980ca8a2d40/crates/sshx/src/main.rs#L15) [E3] |
| Rex | 没有服务器；默认只听本机 Unix socket | 服务端命令行或 App「Add Remote Host…」填 `ts.net` 地址 | 文档 [E2] |

## 直连和换路（源码）

| 产品 | 先直连？ | 连上后还会换到更好的路吗 | 证据 |
|---|---|---|---|
| Tailscale | 是 | 会：有流量时每 3 秒心跳，不是直连就继续探测，已直连也每分钟再试，`betterAddr` 更好就切 | [endpoint.go#L1006-L1022](https://github.com/tailscale/tailscale/blob/4290c03d37d0956e6ab0421bbe85fdc764ff1c50/wgengine/magicsock/endpoint.go#L1006)、[#L2005-L2014](https://github.com/tailscale/tailscale/blob/4290c03d37d0956e6ab0421bbe85fdc764ff1c50/wgengine/magicsock/endpoint.go#L2005) [E3] |
| iroh | 是 | 会：每 60 秒检查能否升级，在同一个 QUIC 连接里换路径、连接不断；中继只做备用 | [remote_state.rs#L48-L66](https://github.com/n0-computer/iroh/blob/f35039493803c240595cc7b56a976b8bb14f8f05/iroh/src/socket/remote_map/remote_state.rs#L48)、[#L653-L685](https://github.com/n0-computer/iroh/blob/f35039493803c240595cc7b56a976b8bb14f8f05/iroh/src/socket/remote_map/remote_state.rs#L653) [E3] |
| Syncthing | 是（局域网 TCP 10 > QUIC 20 > 公网 TCP 30 > 公网 QUIC 40 > 中继 50） | 会：只拨比当前更好的方式，连上后关掉差的 | [optionsconfiguration.go#L77-L82](https://github.com/syncthing/syncthing/blob/b34734376128492fa4e1b01412193de033e20613/lib/config/optionsconfiguration.go#L77)、[service.go#L1411-L1418](https://github.com/syncthing/syncthing/blob/b34734376128492fa4e1b01412193de033e20613/lib/connections/service.go#L1411) [E3] |
| RustDesk | 是：TCP 打洞、UDP 打洞、IPv6、WebRTC 同时竞速，全败才中继 | 源码里没找到（查了 `src/` 的 upgrade/switch/direct/p2p）；直连失败会记下，下次缩短打洞时间 | [client.rs#L1468-L1552](https://github.com/rustdesk/rustdesk/blob/55786aaa5709ac0a450aabcf06f9d1fee3119908/src/client.rs#L1468) [E3] |
| Happy | 否，永远经服务器 | — | 查了 app、cli、wire 里的 webrtc/p2p/mdns，只有语音用 WebRTC [E3] |
| Paseo | 直连是主路（TCP、Unix socket、管道、ssh），中继可选 | 源码里没找到自动切换 | [host-connection.ts#L26-L44](https://github.com/getpaseo/paseo/blob/67628b48fe4c6ec05fbaccca7be4573605c5c5b8/packages/app/src/types/host-connection.ts#L26) [E3] |

iroh 文档说「roughly 9 out of 10」的网络能直连，源码里没有这类数字（全仓搜过），只是文档说法。

## 归纳

1. **两条路线**：一是自己做中继，同时尽量直连（RustDesk、Syncthing、iroh、Tailscale，Paseo 半个）；二是完全不做中继，交给 Tailscale（Rex）。Rex 是和 Mooring 最像的产品，它选了第二条。
2. **官方中继默认开还是关，分两派**：Happy、RustDesk、Syncthing、upterm、sshx 默认用官方；Paseo 新装默认关，要 `--relay` 才开；Rex 根本没有。
3. **中继看不到内容**：读过源码的 8 家里，DERP、iroh、Syncthing、Paseo、sshx、Happy（内容层）做到了，RustDesk 有退回明文的路径，upterm 能读内容。
4. **中继能看到的元数据各不相同**：最少的是 DERP（目标公钥 + 密文）；Happy 存了时间戳等明文元数据；Syncthing 会公开字节数。
5. **连上后继续找更好的路**：Tailscale、iroh、Syncthing 做了；RustDesk、Paseo 源码里没找到。
6. **密钥不经服务器**：Paseo 和 sshx 都把密钥放在链接 `#` 后面。
7. **断线续传**：Rex（兼容实现）按字节偏移补发，不重新要整屏。

## 关键事实核验

| 事实 | 影响哪条结论 | 来源与日期 | 结果 |
|---|---|---|---|
| Paseo 中继「首次启动会问」 | 第一版把它当「问用户」的例子 | 源码默认 `relay.enabled: false`（[persisted-config.ts#L382](https://github.com/getpaseo/paseo/blob/67628b48fe4c6ec05fbaccca7be4573605c5c5b8/packages/server/src/server/persisted-config.ts#L382)）；2026-11-04 | 更正：新装默认关，要 `--relay` 才开 |
| sshx 不能自建 | 第一版归到「只有官方」 | 源码有 `--server` / `SSHX_SERVER`（[main.rs#L15](https://github.com/ekzhang/sshx/blob/3604e8ec5ce0e741bd4a7de2e339b980ca8a2d40/crates/sshx/src/main.rs#L15)） | 更正：代码里可以自建，README 说「not supported」 |
| upterm 中继能不能读内容 | 「中继看不到内容」的反例 | [sshstock.go#L281](https://github.com/owenthereal/upterm/blob/87f8ee86fb34a7a9ca8e0f0e9f34a0189758a0da/server/sshstock.go#L281)、[#L395](https://github.com/owenthereal/upterm/blob/87f8ee86fb34a7a9ca8e0f0e9f34a0189758a0da/server/sshstock.go#L395)：中继两头各自做 SSH | 成立：能读 |
| RustDesk 一律加密 | 「中继看不到内容」 | [client.rs#L1689-L1700](https://github.com/rustdesk/rustdesk/blob/55786aaa5709ac0a450aabcf06f9d1fee3119908/src/client.rs#L1689)：验签失败退回明文 | 更正：有明文退路 |
| Happy 手机端默认官方服务器 | 「官方默认」 | [serverConfig.ts#L9](https://github.com/slopus/happy/blob/c2f3801a9d64ff93249c0d262909f052f1201381/packages/happy-app/sources/sync/serverConfig.ts#L9) | 成立：`api.cluster-fluster.com` |
| Rex 有没有官方中继 | 「Rex 走 Tailscale 路线」 | 官方远程访问文档只列 Tailscale、tailnet、明文 HTTP、自带 VPN；Superlogical 在 GitHub 没有公开仓库 | 在已查范围内没有；Rex 闭源，不能读源码确认 |
| tuxosaurus 能代表 Rex | Rex 的 tsnet、准入、续传结论 | 它自述按 Rex 服务端 revision `a601bcd3` 跑同一套协议测试；不是 Superlogical 的代码 | 只能算兼容实现的证据，文中都标了 |
