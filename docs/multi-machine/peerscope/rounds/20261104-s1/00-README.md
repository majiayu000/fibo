# Mooring 连接方式调研（2026-11-04，源码版）

**结论**：读了 8 个开源产品的源码之后，结论有三处变化。

1. **和 Mooring 最像的 Rex（Ghostty 作者的新终端）没有自己的中继**，官方文档只给 Tailscale 和自带 VPN 的连法。按它的第三方兼容服务端看，服务端内置 Tailscale 节点、按 Tailscale 账号放行。所以「Tailscale」选项可以学它做成内置：被连的电脑不用装 Tailscale，但去连它的手机或电脑仍要装并登录。
2. **官方中继默认开不开，同类分两派**：Happy、RustDesk、Syncthing、upterm、sshx 默认用官方，Paseo 新装默认关。
3. **值得补断线续传**：按兼容实现，Rex 断线重连时按「已收到多少字节」补发，不重新要整屏。Mooring 每次重连都要整屏。注意它只省重连，**第一次打开仍要整屏**，所以救不了今天打开远程画面慢（那是网络一个来回 400 ms）。

建议的连接方式四选一：**Tailscale（内置）/ Mooring 官方中继 / 自建中继 / ssh**。最先做的仍是「显示走哪条路、一个来回多久」。

- [调研：16 个候选、Rex 深挖、9 种中继的内部构造、源码核验](01-competitor-research.md)
- [方案：四个选项、七件事、待定项](02-my-plan.md)

## 待你决定

- 默认用哪个：已登录 Tailscale 的机器默认走 Tailscale，否则默认走官方中继？还是像 Paseo 那样，中继默认关、要你点开？
- Tailscale 选项是内置（tsnet，学 Rex），还是先只让用户填 tailnet 地址。
- 官方中继放在哪、用什么域名、谁出机器（部署要你授权）。

## 覆盖率和没做到

- 候选 16 个。读了固定版本源码的有 8 个：RustDesk、Tailscale、Syncthing、iroh、Happy、Paseo、upterm、sshx。
- Rex 闭源：只读了官方文档，以及一个第三方开源兼容服务端（tuxosaurus）的源码。凡是「Rex 怎么实现」的结论，都标了「兼容实现」。
- VS Code 隧道、Claude Remote Control 闭源，只有文档。
- 没有编译、运行或抓包；没测延迟和打洞成功率。
- tmate、ZeroTier、frp 没细看；没看用户评论。
- 和第一版只读文档相比，按源码更正了：Paseo 中继新装默认关（不是「首次问」）；sshx 代码里能自建；upterm 的中继能读内容；RustDesk 验签失败会退回明文；iroh「约九成能直连」只是文档说法，源码里没有。
- 第一版经过两轮第三方审查；源码版又审了一轮，改正了 Rex 结论没标「兼容实现」、把续传说成能救首屏慢、「不用装 Tailscale」说过头这几处。
