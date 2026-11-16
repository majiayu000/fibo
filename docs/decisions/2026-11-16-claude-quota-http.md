---
status: accepted
date: 2026-11-16
decision-makers: majiayu000
---

# Claude 额度集中通过 HTTP 查询，工作会话不再发布账号额度

## Context

用户看到 Fibo 的 Claude 额度在 19% 和 10% 间跳动，连续追问多个旧额度、额外进程开销、怎样保证最新、重置怎么办，并要求回看原讨论和比较其他软件。比较后用户授权：「你做吧 把这些都要记录下来 把你调研的」。

本机已复现同一重置时间下 19→10→19、22→18→22、23→17→23。多个已经在工作的 Claude 会话各自缓存不同读数，状态栏重绘把它们交替写进同一个文件。文件 mtime 只能证明脚本刚运行，不能证明服务端额度刚被采集。当前安装的旧插件是 0.1.3；它没有为了取额度额外启动 Claude。

原始沿革：10-31/11-02 用本地记录，11-03 改成插件，11-07 参考 magpie 增加默认关闭的 `/usage` 查询并改成 Rust。直接 HTTP 是当时实现者否掉的方案，已找到的用户原文没有禁止 HTTP。旧决定见 [11-07](2026-11-07-claude-usage-read.md)，本决定取代其额度来源和调度部分。

## Decision

要支撑的决定：是否采用一个集中 HTTP 入口取代多会话状态栏额度和完整 CLI 启动。指标是读数不倒灌、账号隔离、原始采集时间、重置处理，以及真实 CPU/内存开销。

- `claude-quota usage` 原生 HTTP GET `https://api.anthropic.com/api/oauth/usage`。不运行 Claude CLI，不发模型请求。
- 额度声明只保留这一个命令来源。状态栏只保存各会话上下文并转发用户原命令。旧 `statusline.json`、`usage.json` 即使还被旧进程写入也不再参与额度选择。
- 正常成功快照缓存 120 秒；现有插件调度每 60 秒读取一次，因此有效刷新最多延迟一个调度周期。窗口重置后的下一次调度提前补查。没有“每一刻都等于服务端”的承诺。
- 进程间 OS 文件锁覆盖读取缓存、判断是否到期、HTTP 和发布。锁忙时只看当前凭据对应的旧快照。进程退出由 OS 释放锁，不靠超时删锁。由于请求串行，没有两个请求倒序发布，不另外增加代次系统。
- 凭据在内存中使用，缓存只保存 SHA-256 指纹。指纹变化视为新范围，旧快照不跨范围复用；请求后再次读凭据，变化则丢弃结果。token 轮转会多查一次，优先避免错误归属，不引入账号映射表。
- macOS 后台读取钥匙串不弹授权框；显式 `fibo claude-quota refresh` 才允许系统授权。其他系统读 Claude Code 自己的凭据文件。自定义目录不回落到全局账号。Fibo 不更新或保存 Claude 的登录令牌。
- 成功才更新采集时间。网络失败保留原快照、原时间并标错误；首次失败、账号变化、401/403 不展示旧账号读数。429 按 Retry-After 等待，手动刷新也不绕过。
- 每个窗口沿用其服务端 reset：过期值显示「已重置／等待新读数」，不伪造 0。5 小时与每周独立。新周期低百分比、同周期服务端合法下降都接受；不采用最大值策略。
- 错误通过插件 quota 的 `error` 字段传给通用额度展示。插件级别 9、claude-quota 0.3.0，旧宿主不能加载新字段。已有旧 App 和旧插件不算此次修复已经生效。

## Alternatives

| 来源／固定提交 | 核读结果 | 采用或否定 |
|---|---|---|
| [QuotaBar 4639c91e](https://github.com/majiayu000/quotabar/blob/4639c91ebded74f7df811584a500bcca6de4fe0b/src-tauri/src/services/claude.rs) | Rust HTTP；120 秒缓存；两个窗口由 commands.rs 的异步锁串行读；429 按 Retry-After。外层失败锁存等显式重查，15 分钟只约束选择旧缓存的时刻 | 采用集中入口、缓存、限流；不照搬失败锁存和未证明的账号隔离 |
| [CodexBar 1f6da9be](https://github.com/steipete/CodexBar/blob/1f6da9bec44455288ddee5ef6f3617a803d8eb2a/docs/claude.md) | App 优先 OAuth，再 CLI、Web；查询代次与账号检查；失败保留 capture 时间；重置边界补查 | 采用来源时间和账号检查；不引入多源凭据体系。串行查询足以解决本次顺序问题 |
| [magpie 9c4e8773](https://github.com/yetone/magpie/blob/9c4e87736cc84bcc1924df06440cad3e7ababee0/internal/provider/subscription_usage.go) | 集中 `claude -p /usage`；普通后台随机 5–15 分钟且使用后才查；临近耗尽加速；按账号缓存；过期窗不再算满；登录/订阅缓存检查查询代次 | 不再采用完整 CLI 启动；不把其本地过期归零作为真实采样值 |
| [herdr-status-ui-bar 07cb567d](https://github.com/jimididit/herdr-status-ui-bar/blob/07cb567dbd27f3bb0a963da50386ef81170e2b63/agent_usage.py) | 被动读共享 statusline，mtime 超过 6 小时标陈旧；未处理账号、reset、采样顺序 | 不用它证明被动共享文件能保证最新 |

「同周期取最大值」被否定：不知道实际采样时间，既无法证明新鲜，也会挡住服务端合法下调。单用 Claude 全局 cachedUsageUtilization 也被否定：本机观测该缓存停在 29%/40% 数分钟，状态栏已到 32%/41%。

## Performance evidence

本轮此前在本机 Claude Code 2.1.325 实测隔离的 `claude -p /usage`：一次 wall 10.10 秒、user+sys 7.40 秒、最大 RSS 448,184,320 字节，模型费用 0。这是 CLI 启动开销，不是状态栏回调开销。历史“Rust 21 ms / shell 49 ms”测的是状态栏回调，也不能用于论证 `/usage` 很轻。

首次真实账号只读运行在 macOS 钥匙串读取处返回权限错误，未发送 HTTP：wall 6.60 秒、user+sys 0.02 秒、最大 RSS 13,828,096 字节。这些只说明失败路径成本，不能作为 HTTP 查询成功后的性能数据。证据：外部 `http-live/query.json`、`http-live/resources.txt`。竞品只核读源码，没有实测其 CPU、内存或当前账号连通性。

## Implementation Plan

- host 的 `agents/claude_quota/{usage,oauth,status}.rs`；平台层只封装现有钥匙串读取。
- 复用现有插件调度、存储目录和额度弹层；不加常驻服务，不扫会话目录来猜最新，不加刷新模式配置。
- 通用 Quota 保存刷新错误，继续由原始 at 计算展示年龄。Codex/Grok 默认无错误。
- 真手机仍沿用相同额度协议；插件或 App 真正更新后必须按手机走查清单复验。

## Verification

已观察到真实二进制的旧状态栏转发、旧脚本接管、上下文分会话保存；HTTP 测试服务器返回的 19%/37% 经真实 reqwest 传输解析，重定向未跟随、错误未泄露令牌。缓存测试回放旧写入和低值、新周期、限流及换凭据；并发测试在写入冷却时间之前停住第一个读取者，第二个仍只能读旧快照。完整检查与界面对照结果在完成后补记。

尚未安装此分支 App 或插件；真实账号 HTTP 被钥匙串权限挡住，未测其成功路径或长时间 429 频率；真手机未验。不能把代码测试当作运行版已修复。

放弃条件：当前账号不能可靠授权读取或接口持续拒绝；无法把账号变化与旧数据隔离；HTTP 实测成本不满足低开销目的。遇到这些情况明示未知，不自动退回旧 statusline 或启动 Claude CLI。

## More Information

- 完整调研回写知识树：`~/Documents/knowledge-base/wiki/entities/monitoring-quota.md`。
- 固定提交原文摘录：`~/Documents/knowledge-base/raw/web/2026-11-16-claude-quota-implementations.md`。
- 本机原始回放、原讨论追溯、资源测量：`~/Documents/claude-pages/fibo/2026-11-16-quota-fluctuation/` 的 report.md、history.md、design-draft.md 和 JSON 证据。design-draft.md 中的被动最大值方案是被取代的草案，不是本次实现规范。
- 原始历史 Claude 会话：669a31c1-c309-4f1c-b75a-0f544e45543d、698ca1a9-b6a7-42e1-a55c-a19a9689b2fb、9452cae9-3552-4df2-ac7c-dbe046b64301。
