---
status: accepted
date: 2026-11-27
decision-makers: majiayu000
---

# Claude 额度回到状态栏读数加可选 /usage，同一周期取最大值；不读钥匙串

## Context

[11-16 的决定](2026-11-16-claude-quota-http.md) 为修 19% / 10% 回跳，把 Claude 额度改成 HTTP 查 `api.anthropic.com/api/oauth/usage`，凭据从钥匙串里的 `Claude Code-credentials` 读。11-27 在 apple 上新装 claude-quota 时，后台读钥匙串被拒（`-25293`），要用户在那台电脑上手动授权，用户这才知道它在读钥匙串：「啥情况 现在使用钥匙串了吗？？」「我记得原来不是这样设计的啊」。

查记录：11-07 的设计（`5f9c0cf8`，[决定](2026-11-07-claude-usage-read.md)）明确否掉过 HTTP：「要碰钥匙串凭据，接口私有」。11-16 由 Codex 会话推翻，提交里写的取代理由是「完整 CLI 查询实测峰值 448 MB」，没有权衡 11-07 的 `/usage` 调度本来就是 5–15 分钟一次、用过才读。用户看了两版：「我记得是这个是最好的 并且我还做了很多的算法优化啊」。

回跳本身 11-16 之前没有修：`quota_merge::newest` 按文件修改时间取最新，最后写文件的会话赢。当天草案提过「同账号同窗口取最大值」，被 HTTP 取代，没有实现。用户确认按草案的思路做：「可以 你做吧」。

## Decision

- 恢复 11-07 的来源：状态栏回调写 `statusline.json`（被动，不联网）；「联网读 /usage」默认关，开了由 Claude Code 自己跑 `claude -p /usage`，沿用原来的调度（5–15 分钟随机、用过才读、≥90% 每分钟、App 和后台用锁只读一次）。代码从 `beedd4da^` 恢复。
- 回跳在写文件时解决（`agents/claude_quota/limits.rs`）：每个窗口按 `resets_at` 比，`resets_at` 更晚的是新周期，直接替换；相同的取 `used_percentage` 大的。实机数据里各会话的 `resets_at` 完全相同（周 `1796101200`，5h `1795780800`），所以按相等判断同一周期。
- 删掉 HTTP 方案：`oauth.rs`、`fibo claude-quota refresh`、`keychain::password`，以及只有它用的 `reqwest`、`sha2`（host）、`security-framework`（platform）。`keychain::AVAILABLE` 留给插件密钥。
- core 的 `Quota.error`、插件级别 9、弹层里「等待读数 / 刷新失败」这些通用能力不动。

不做：

- **HTTP 当可选项留着**：要读钥匙串，正是这次用户不认的那一点；要的时候再按 11-16 的代码恢复。
- **在 core 读所有会话的 `contexts/*.json` 取最大**：core 只按插件声明读一个文件，改读法要动通用格式；写文件时合并只改插件自己。
- **给写文件加锁**：两个状态栏同一毫秒读改写，偶尔会让较小的值赢一次，下一次较大的会话刷新就回来，不值得多一把锁。

## Consequences

- 没覆盖的：状态栏数据里没有账号字段，两个账号的会话同时开着会显示较大的那个；服务端在同一周期内往下修正，要等重置才显示。
- 读数的「多久前」仍是文件写入时间，不是服务端采样时间。
- 已装 0.3.0 的：插件 0.4.0 比它新，App 启动时自动换上（#332 的路径）；0.3.0 留下的 HTTP 缓存文件不再有人读。0.3.0 期间没写 `statusline.json`，所以换上后要等下一次状态栏刷新才有读数。
- 旧 App 配新插件：插件级别 8，0.3.0 以前的 App 都能装。

## Verification

- `cargo test -p fibo-host --test claude_quota an_idle_conversations_old_limits_do_not_pull_the_reading_back`：真程序按 19 → 10 → 22 → 新周期 2 → 旧周期 22 喂状态栏，文件里依次是 19、19、22、2、2；撤掉合并这条会失败。
- `cargo test -p fibo-host --lib claude_quota`（含 `limits::tests`）、`cargo test -p fibo-core --lib quota`。

## More Information

- 重开条件：多个账号同时用成了常态，或者同周期往下修正成了常见现象。
- 来源提交：`5f9c0cf8`（11-07）、`beedd4da`（11-16，被本决定取代来源和调度部分）。
