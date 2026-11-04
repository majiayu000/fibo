# Mooring 连接方式调研（2026-11-04）

**结论**：同类产品里自己有服务器的，常见做法是「官方服务器默认、设置里能换成自建、能直连就不走服务器」（RustDesk、Syncthing、iroh 写明了这三点），中继也大多看不到内容。Mooring 建议照这个做：一个「连接方式」设置，四个选项——

1. **自动（默认）**：先直连，不通就走官方中继，走中继期间继续试直连；
2. Mooring 官方中继；
3. 自建中继（填地址和 key）；
4. 只走 ssh / Tailscale（现在的方式）。

最先该做的不是选项本身，而是**显示当前走哪条路、一个来回多久**。今天 apple 那条 Tailscale 路一个来回 400 ms，界面上看不出来。

- [调研：15 个候选、8 家怎么做、事实核验](01-competitor-research.md)
- [方案：四个选项、要做的五件事、待定项](02-my-plan.md)

## 待你决定

- 默认是不是「自动」；直连不通时默认走官方中继，还是第一次先问你（Paseo 的做法）。
- 官方中继放在哪、用什么域名、谁出机器（部署要你授权）。
- 直连先做简单版（试已知的局域网和 Tailscale 地址），还是直接做打洞（iroh、嵌入 Tailscale、mDNS 还没比较）。

## 覆盖率和没做到

- 候选 15 个，深挖 8 个：RustDesk、Tailscale、Syncthing、iroh、Happy、Paseo、VS Code Remote Tunnels、Claude Code Remote Control。另有 upterm、sshx 只读了 README。
- 全部只读了官方文档和 README，没读源码，也没装上实测延迟和打洞成功率。
- tmate 官网没打开，ZeroTier、frp 没细看。
- 没看用户评论。
- Paseo 默认开不开中继、Happy 手机端默认用哪台服务器，文档没写清。
- 经过一次第三方审查：改正了 Happy 设置项的名字和默认值，补上「中继兜底要不要先问你」这个待定项。
