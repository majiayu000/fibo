# 连接方式：同类产品怎么让两台设备连上（2026-11-04）

图例（证据等级）：[E2] 只看了官方文档或 README；[E3] 读过源码；[D1] 只有零星讨论。这一轮全部是 [E2]，没有装任何产品实测延迟。

## 证据边界

| 来源 | 能支持 | 不能支持 |
|---|---|---|
| 各家官方文档、README（2026-11-04 读） | 有哪些连法、默认哪个、用户在哪里改、是否端到端加密 | 实际延迟、打洞成功率、界面实际长什么样 |
| GitHub 仓库数据（星数、最近推送、许可证） | 采用度、是否还在维护 | 用户满意度 |

没做：装上实测、读源码核对加密实现、看用户评论。

## 候选池（15 个）

| 产品 | 做什么 | 采用度 | 连法 |
|---|---|---|---|
| [RustDesk](https://github.com/rustdesk/rustdesk) | 远程桌面 | 128,104★，AGPL-3.0 | 官方服务器（默认）/ 自建 / 自己写服务器；先打洞，失败才中继 |
| [Tailscale](https://tailscale.com) + [Headscale](https://github.com/juanfont/headscale) | 组网 | Headscale 45,343★ | 先直连，不行走 peer relay，再不行走官方 DERP；可自建 DERP、自建控制面 |
| [Syncthing](https://github.com/syncthing/syncthing) | 文件同步 | 90,053★，MPL-2.0 | 直连优先，公共中继兜底，可指定自己的中继 |
| [iroh](https://github.com/n0-computer/iroh) | Rust 点对点库 | 12,921★，Apache-2.0 | 打洞（约九成直连），不行走中继；默认 n0 公共中继，可付费独享或自建 |
| [VS Code Remote Tunnels](https://code.visualstudio.com/docs/remote/tunnels) | 远程开发 | — | 只走微软 dev tunnels，要登录 GitHub/微软账号；另有 Remote-SSH |
| [Claude Code Remote Control](https://code.claude.com/docs/en/remote-control) | 手机/网页接本机 Claude 会话 | — | 只走 Anthropic API |
| [Happy](https://github.com/slopus/happy) | 手机遥控 Claude Code | 24,464★（11-03） | 官方公共服务器（桌面端没配置时用它）或自建，端到端加密 |
| [Paseo](https://github.com/getpaseo/paseo) | 多端遥控 Agent | 22,879★（11-03） | 直连（TCP/Tailscale/VPN）或端到端加密中继，首次启动时问 |
| [Omnara](https://github.com/omnara-ai/omnara) | 托管的 Agent 遥控平台 | 3,349★ | 走它的平台；自述没有端到端加密（[D1]，见 11-03 那一轮） |
| [upterm](https://github.com/owenthereal/upterm) | 终端共享 | 1,332★，Apache-2.0 | 默认公共服务器，`--server` 指向自建 |
| [tmate](https://github.com/tmate-io/tmate) | 终端共享 | 6,135★ | 默认官方服务器，可自建（本轮官网没打开，未核实细节） |
| [sshx](https://github.com/ekzhang/sshx) | 网页终端共享 | 7,711★，MIT | 只有官方服务器，「Self-hosted deployments are not supported」 |
| [ZeroTier](https://github.com/zerotier/ZeroTierOne) | 组网 | 17,232★ | 官方根服务器，可自建（未细看） |
| [frp](https://github.com/fatedier/frp) | 内网穿透 | 110,434★，Apache-2.0 | 只能自建服务器（未细看） |
| ssh + Tailscale/局域网 | 替代物：什么都不加 | — | Mooring 现在桌面连桌面就是这样 |

## 深挖：每家怎么把选择交给用户

**RustDesk**：README 原话是可以用「our rendezvous/relay server, set up your own, or write your own」[E2]。
- 改法：客户端主页 ID 旁的「⋮」→「网络」，填「ID 服务器」「中继服务器」「Key」三格。中继可以留空，它会自己推出来；Key 是自建服务器的公钥，用于加密连接（[文档](https://rustdesk.com/docs/en/self-host/client-configuration/)）[E2]。
- 连法：先打洞，打不通才走中继；文档说打洞「succeeds in most cases」（[文档](https://rustdesk.com/docs/en/self-host/)）[E2]。
- 只开 443 时只能走 WSS 中继，没有直连 [E2]。

**Tailscale**：大多数连接只用 DERP 来建立直连（原文「Most connections … only use DERP servers to establish a direct connection」）；能直连就直连，不能就先试 tailnet 里自己配置的 peer relay（要事先设置），最后才走官方 DERP（[文档](https://tailscale.com/kb/1232/derp-servers)）[E2]。
- 加密：经过 DERP 的流量是 WireGuard 加密的，私钥不离开设备，DERP 解不开 [E2]。
- 自建：可以自建 DERP，但文档说这是少数情况、维护成本高，并推荐 peer relay；控制面可以用 Headscale 自建 [E2]。
- 这就是今天量到的情况：apple 那条 ssh 走 Tailscale，一个来回约 400 ms，推测是没直连、走了中转——本机没装 `tailscale` 命令行，没用 `tailscale ping` 确认（11-04 实测：ssh 上一个来回中位 398 ms，两台电脑本身处理 1–5 ms）。

**Syncthing**：中继默认开着，只在直连不通时用；走中继期间会定期重试直连，一通就不再用中继（[文档](https://docs.syncthing.net/users/relaying.html)）[E2]。
- 自建：在监听地址里写 `relay://<host>/?id=<中继设备 ID>` 就能指定自己的中继 [E2]。
- 加密和元数据：中继只转发加密数据，但能看到 IP、设备 ID 和流量大小；文档把这一点写明了 [E2]。

**iroh**（Rust 库，Mooring 可以直接用）：中继不存状态，只在直连打通前转发，也帮两端交换 NAT 信息。文档说「roughly 9 out of 10」的网络能直连（[文档](https://docs.iroh.computer/concepts/relays)）[E2]。
- 默认中继：写死了一组 n0 的免费公共中继，有限速、不保证可用，文档说只适合开发测试 [E2]。
- 生产环境：用他们的付费独享中继或自建开源中继；中继看不到内容 [E2]。

**Happy**（最像 Mooring 手机端的同类）：
- 官方服务器：`happy-api.slopus.com`，免费（[happy-server](https://github.com/slopus/happy-server)）[E2]。
- 自建：手机 App 登录前首屏齿轮 →「Server」→「Server URL」（已登录要进开发者模式改 API Endpoint）；CLI 用环境变量 `HAPPY_SERVER_URL`；桌面端依次看环境变量、配对时存的服务器、导入的登录，都没有才用「Happy's public server」（[自建指南](https://happy.engineering/docs/guides/self-hosting)）[E2]。
- 加密：服务器「only encrypted blobs it cannot read」[E2]。

**Paseo**：服务器版首次启动时问要不要开「end-to-end encrypted relay」；不开就「connect directly over TCP, Tailscale, or another VPN」。手机在桌面端「设置 → 主机 → Pair Device」里配对（[README](https://github.com/getpaseo/paseo)）[E2]。另有一个 Elixir 写的「official distributed relay」仓库；默认选哪个、怎么自建，README 没写 [E2]。

**VS Code Remote Tunnels**：两端都向微软的 dev tunnels 发出连接，所以不用开端口；必须登录同一个 GitHub 或微软账号。隧道里再套一层 SSH，做端到端加密（[文档](https://code.visualstudio.com/docs/remote/tunnels)）[E2]。文档没提怎么自建中继，想不经过微软就用另一个扩展 Remote-SSH。

**Claude Code Remote Control**：本机只发出 HTTPS 请求，向 Anthropic API 注册并等任务，「All traffic travels through the Anthropic API over TLS」（[文档](https://code.claude.com/docs/en/remote-control)）[E2]。全文没有出现 end-to-end，也没有自建的说法。

**upterm / sshx**：
- upterm 默认连 `uptermd.upterm.dev`，`--server` 指向自建，`--authorized-keys` 限定谁能注册；没有点对点直连（[README](https://github.com/owenthereal/upterm)）[E2]。
- sshx 只有官方服务器，端到端加密（Argon2 + AES）；会挑近的服务器，并显示实时延迟估计（[README](https://github.com/ekzhang/sshx)）[E2]。

## 归纳：四种做法

| 做法 | 谁 | 用户怎么选 |
|---|---|---|
| 官方服务器默认、可换成自建、尽量直连 | RustDesk、Syncthing、iroh、Tailscale | 不用选就能用；高级设置里填自建地址和 key |
| 官方默认 + 自建，没有直连 | Happy、upterm、tmate | 设置里一个「服务器地址」 |
| 只有官方 | VS Code 隧道、Claude Remote Control、sshx | 不能选；要账号或靠厂商 |
| 不给服务器，用户自己组网 | frp、Paseo 的「拒绝中继」、Mooring 现在的 ssh | 自己保证两边能直接连上 |

共同点：
1. **有官方服务器的产品，文档写明默认用它的有 RustDesk、upterm、Syncthing、iroh，Happy 桌面端也是**（手机端没写）。自建都是在设置里换一个地址，不是另一套流程 [E2]。
2. **中继看不到内容**：抽查的 5 家（Tailscale、iroh、Syncthing、Happy、Paseo）都写明了，tmate、upterm 没查；VS Code 在隧道里再套 SSH。Claude Remote Control 是例外，只写了 TLS [E2]。
3. **能直连就不走中继**（Syncthing、Tailscale、iroh、RustDesk 都这么写）；走中继期间继续试直连、通了就切过去，只有 Syncthing 的文档写明了 [E2]。
4. **把元数据讲清楚**：Syncthing 明说中继能看到 IP、设备 ID 和流量大小 [E2]。
5. **官方公共中继有限速**：iroh 明说限速、只适合开发 [E2]。官方服务器的容量和费用要提前想好。
6. **显示网络状况**：sshx 显示实时延迟估计 [E2]。

## 关键事实核验

| 事实 | 影响哪条结论 | 来源与日期 | 结果 |
|---|---|---|---|
| Claude Remote Control 没有端到端加密 | 「中继看不到内容是标配，Claude 例外」 | 反查：在官方文档全文搜「end-to-end」，0 处；只写「over TLS」；2026-11-04 | 成立，写成「没写端到端」，不说「没有」 |
| sshx 不能自建 | 归到「只有官方」 | README 原话「Self-hosted deployments are not supported at the moment」；2026-11-04 | 成立 |
| RustDesk 官方服务器是默认 | 归到「官方默认」 | README 原话「Works out of the box with no configuration required」「use our rendezvous/relay server」；2026-11-04 | 成立 |
| Happy 默认用官方服务器 | 同上 | 官方自建指南：桌面端没配置时用「Happy's public server」；手机端默认值没写；2026-11-04（审查员核对） | 部分成立：桌面端成立，手机端未核实 |
| Paseo 默认开中继 | 归类 | README 只说首次启动会问，没写预选哪个 | 无法核实，归到「问用户」 |
| iroh 约九成能直连 | 「加上打洞能少走中继」 | 官方文档原话「roughly 9 out of 10 networking conditions」；2026-11-04 | 成立（是厂商自述，未实测） |
