# Mooring 的连接方式：怎么设计（2026-11-04，按源码修订）

要支撑的决定：Mooring 给用户哪几种连接方式、默认哪个、在哪里选、各自要做多少事。
放弃条件：官方中继两周内拿不到一台机器，就先做「Tailscale + 自建中继」，官方中继往后放。

## Mooring 现在有什么

| | 电脑连电脑 | 手机连电脑 |
|---|---|---|
| 走法 | 只有 ssh（用户自己保证能连，比如 Tailscale） | 只有自建中继 `mooring-relay`，配对链接带中继地址 |
| 加密 | ssh | Noise 端到端（和 Paseo 同类做法）；每台手机独立路由码 |
| 断线后 | 重新 `watch`，整屏重发（今天打开一次要 1.1–1.3 秒） | 同左 |
| 官方服务器 | 没有 | 没部署 |

## 源码看下来，改变了什么

1. **最像 Mooring 的 Rex 没有自己的中继**，把 Tailscale 直接嵌进服务端（tsnet），身份就是 Tailscale 账号，默认只放主人。这说明「Tailscale 选项」可以做得比「让用户自己配 ssh」好得多：不用装 Tailscale 客户端、不用配 ssh、不用配对码。
2. **官方中继默认开不开，同类分两派**：Happy、RustDesk 默认开；Paseo 新装默认关、要 `--relay`。
3. **断线续传比加快首屏更值钱**：Rex 按「已收到多少字节」补发，60 秒内不用重新要整屏。Mooring 现在每次重连都要整屏。
4. **好的中继都在连上后继续找直连**（Tailscale 3 秒心跳、iroh 每 60 秒、Syncthing 按优先级），而且换路时连接不断（iroh 在同一个 QUIC 连接里换）。
5. **反面例子**：upterm 的服务器会解开 SSH、能读内容；RustDesk 验签失败会退回明文。Mooring 的中继要保持「解不开」，握手失败就断开，不退回明文。

## 建议：连接方式四选一

设置 › 手机与远程 ›「连接方式」，每台电脑、每台手机可以单独改：

| 选项 | 做什么 | 参照 | 谁能看到什么 |
|---|---|---|---|
| **Tailscale（推荐给已有 Tailscale 的人）** | Mooring 后台用 tsnet 作为一台独立设备加入你的 tailnet；对方按 Tailscale 账号放行，默认只放你自己 | Rex | Tailscale 的 DERP 只看到公钥和 WireGuard 密文 |
| **Mooring 官方中继** | 走你部署的 `mooring-relay`；配对码 + 路由码，不用账号 | Happy、Paseo | 看不到内容；能看到两端 IP、路由码哈希、时间、字节数 |
| **自建中继** | 同上，换成你自己的地址和可选的注册 key | RustDesk、Happy、sshx | 同上，只是服务器是你的 |
| **ssh**（现在的方式） | `ssh 主机 mooring remote --stdio` | Paseo 的 remoteSsh、VS Code Remote-SSH | 由 ssh 决定 |

默认值待定，见文末。不论选哪个，跑的都是同一套远程协议（`crates/mooring-remote`）：现在 ssh 那条路跑的就是 `mooring remote --stdio`，其他选项只是多几种传输方式。

## 要做的事，按顺序

1. **显示走哪条路、一个来回多久**：侧栏机器标题写「Tailscale · 400 ms」「官方中继 · 60 ms」。小，马上能做。
2. **断线续传**（学 Rex）：`watch` 带上「已收到多少字节」，后台在一段时间、一定字节内补发，不重新要整屏。打开偷看窗也可以先显示上次的画面（之前方向稿的 A）。
3. **桌面连桌面也走中继**（多机第 5 步）：复用第 4 阶段的设备配对。
4. **Tailscale 选项用 tsnet**：后台作为独立 tailnet 设备，按 WhoIs 账号放行。tsnet 是 Go 库，Mooring 是 Rust，要么嵌一个小 Go 辅助进程，要么用 libtailscale 的 C 接口；先做一个可行性试验，量体积和启动时间。
5. **连上后继续找更好的路**：走中继时每分钟试一次已知的局域网 / Tailscale 地址，通了就换（学 Syncthing、Tailscale）。完整打洞（iroh）等前面做完再评估。
6. **官方中继**：部署 `mooring-relay`，要你授权和机器。先定机房（国内用户多就放国内）、限速、带宽费用。不加账号。
7. **自建中继说明页**：一条 `docker run`，加上设置里填的两个值。

## 不建议做

- 自己做打洞协议：要么用 Tailscale（tsnet），要么用 iroh，不自己写。
- 中继退回明文、中继解开内容（RustDesk、upterm 的做法）。
- 账号体系：Tailscale 选项用 Tailscale 账号，中继选项用配对码，Mooring 不另做账号。

## 待你决定

1. 默认用哪个：已登录 Tailscale 的机器默认走 Tailscale（Rex 的做法），否则默认走官方中继？还是像 Paseo 一样，中继默认关、要你点开？
2. Tailscale 选项做不做 tsnet 内置（第 4 步），还是先只让用户填 tailnet 地址。
3. 官方中继放在哪、用什么域名、谁出机器。
