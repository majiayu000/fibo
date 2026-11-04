# 审查 1（第三方审查 agent，2026-11-04）：需修改

抽查 9 条，8 条成立；Happy 设置名和默认值不成立。

| 意见 | 处置 |
|---|---|
| Happy 设置名是 Server → Server URL；HAPPY_SERVER_URL 在官方指南里 | 已改 01、HTML |
| Happy 默认值前后矛盾；官方指南写桌面端没配置时用公共服务器 | 统一为「桌面端默认、手机端未核实」 |
| Tailscale「所有连接」说重了 | 改为「大多数连接」并引原文 |
| 漏了「中继兜底默认开还是先问」 | 加进 02、00、HTML 的待定项 |
| 「标配」只抽了 5 家 | 改为「抽查的 5 家都写明了」 |
| 400 ms 走中转是推断 | 写明是推测，本机没有 tailscale 命令行可验 |
| 官方中继能看到什么是套 Syncthing | 按 mooring-relay 实现改写 |
| 手机直连要开监听端口 | 02 加一段 |
| 「远程协议共用」没出处 | 02 补 crates/mooring-remote 和 remote --stdio |
| 直连只比了 iroh | 02 加 tsnet / mDNS 候选，标为未比较 |

# 审查 2（新的审查 agent）：需修改 → 已改

| 意见 | 处置 |
|---|---|
| peer relay 是 tailnet 里自己配置的设备，不是同一局域网 | 01、HTML 已改 |
| Happy 改服务器的入口和桌面端回退顺序 | 01、HTML 补半句 |
| HTML「没做到」与 00 不一致 | 已统一 |
| HTML 里 Happy 加密说明的来源 | 加 happy-server 链接 |
