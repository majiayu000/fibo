---
status: accepted
date: 2026-11-07
decision-makers: majiayu000
---

# Claude 额度可以让 Claude Code 自己查 /usage，默认关

## Context

Mooring 的 Claude 额度只来自状态栏：Claude 不刷新状态栏就不更新，也拿不到按模型的周额度（Fable）。用户问能不能学 yetone/magpie 的额度。magpie 不调接口，跑 `claude -p /usage`。

## Decision

`claude-quota` 插件加一条命令来源和设置项「联网读 /usage」，默认关。

- **「Mooring 不联网」的边界：** Mooring 自己仍不问任何厂商。这条命令是 Claude Code 在联网，所以插件 `network` 改成 true，弹层文案不再写「不联网」。
- **隔离：** `--no-session-persistence --setting-sources "" --tools "" --strict-mcp-config`。不加会留会话记录、跑 hooks，定时跑就是侧栏里一排假会话。
- **频率：** 5 到 15 分钟随机，且这段时间用过 Claude 才读，快用完时（账号自己的窗口 ≥90%）每分钟。App 和后台各跑一份，靠锁只读一次。平时 6 秒，机器忙时（load 250）实测 14 到 23 秒，所以这条来源的超时是 90 秒。
- **两份读数：** 还是显示最新的，1 小时内其他来源独有的带名字窗口留在旁边，否则 Fable 会随状态栏刷新一闪一闪（`quota_merge`）。
- **读不到：** 退出码非 0，保留上次读数；只有 Claude Code 明确只给费用摘要（API key、未登录）才算没有额度。

不做：


- **直接调 `oauth/usage` 接口：** 要碰钥匙串凭据，接口私有。
- **交互模式读 `/usage`：** 启动时发一次计费请求，会打开闲置账号的 5 小时窗口（magpie #5582）。
- **依赖 magpie：** 用户没装，走它的网关后状态栏可能没有额度。
- **把解析放进 core：** 新来源写成插件（`quota.rs` 头注释），core 也不能写具体 CLI。所以放在 host 的 `agents/claude_quota`，插件只声明调用哪个 `mooring` 子命令。
- **解析写成 JXA、调度写成 sh：** 写过一版，用户问「这不是 Rust 吗」后撤掉。JXA 只能在 macOS 跑，还因为 JXA 的数字拼接把日期算错过。插件原有的 `settings.js`、`statusline.sh` 也一起改成 Rust，用户定「一次改干净」。
- **发给手机：** 手机每个 Agent 只有一行「本周」，按窗口长度当 key，旧手机 App 会显示两个「本周」。host 只发账号自己的窗口。要手机显示时要走真机。

## Consequences

放弃条件：连续两周 Fable 周额度都接近 0，或者用户没开过这个开关，就把命令来源撤掉，只留状态栏。11-21 复查。

风险：`/usage` 是给人看的文字，不是接口，改了格式就读不懂：会报错并保留旧读数，不会显示错的数。magpie 的注释提到过 usage 页面会被限流，频繁读的情况没实测。

## Implementation Plan

受影响：`crates/fibo-host/src/agents/claude_quota/`（`mooring claude-quota status|usage|setup|teardown`）、`plugins/claude-quota`（不再带脚本，`versions/0.1.3` 留给旧 App）、`crates/fibo-core/src/quota.rs`、`quota_merge.rs`、插件级别 8。

Windows：解析和设置都是 Rust，时区库打包进去，但没有在 Windows 上跑过；host 进 Windows CI 之前，这些测试只在 macOS 和 Linux 跑。

## Verification

`crates/fibo-host/tests/claude_quota.rs` 用假 `claude` 真跑程序，`agents/claude_quota/usage_text_tests.rs` 用真实输出测解析，`quota_merge::tests` 管合并。
