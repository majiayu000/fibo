# 窗口里显示分支的 PR 和 CI 状态

状态：方案，待用户拍板，未改代码。对应 [#602](https://github.com/majiayu000/fibo/issues/602)（11-26 竞品对照，汇总在 #621）。

## 要支撑的决定

- 窗口里要不要显示会话所在分支的 PR 和 CI 状态；数据怎么取（这会让 Fibo 多一类联网），显示在哪；要不要提供「开 PR」之类的动作。
- 指标：从「等你验收」到「完成」之间，不用切到浏览器就能知道 PR 开了没、CI 过了没；同时不打破 README 对联网的承诺。

## 现状（2026-11-28，main 3075da01）

- 仓库里没有任何调用 `gh` 或 GitHub API 取 PR 的代码（自更新用的是 GitHub Releases，在 `fibo-platform` 的 `app_bundle`）。
- 分支：窗口在后台读本机目录的分支并缓存（`workbench/layout/turn_bar.rs` 的 `git_branch`，按 `BRANCH_TTL` 过期）；手机卡片的 `branch` 由 host 给。看板卡片上只有分支名。
- 联网承诺（README 的 FAQ）：App 自己发起的联网只有每天一次的更新检查，其余联网都要等用户触发（插件目录、webhook、用户加的 SSH 机器、用户装的插件）。
- 多机（[11-14 决定](../decisions/2026-11-14-one-window-many-hosts.md)）：窗口直连别的电脑的 host。PR 查询要在终端所在那台、用那台的 `gh` 登录状态来跑。
- PATH：host 给终端设系统最小 PATH，再由登录 shell 的配置补全（`fibo-platform/src/shell/unix.rs`）。host 进程自己拿不到用户的 PATH，Homebrew 装的 `gh`（`/opt/homebrew/bin`）它不一定找得到。

## 可选方案

### A host 按需调 `gh`，只在用户看的时候查（推荐）

- 新请求 `Request::PrStatus { terminal }`：host 在 `terminal_cwd` 里跑 `gh pr view --json number,url,state,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup`（当前分支），10 秒超时。`gh` 不在、没登录、不是 GitHub 仓库、分支没有 PR，都回「无」，不报错。
- 查的时机：鼠标停在会话行的分支上、打开看板卡片、打开「看改动」时各查一次，结果缓存 60 秒。没有后台轮询，每次联网都由用户的动作触发，守住 README 的承诺。
- 显示：看板卡片的分支旁边显示 `#123` 和检查汇总（✓ / ✗ / 进行中）；侧栏行在悬停提示里显示；点击在浏览器打开 PR。
- 设置里加开关「在窗口里查 PR 状态（用这台电脑的 gh）」，默认关；关着时一次都不查。

### B 后台定时轮询

- 更「实时」，但打破 README 的承诺；N 个会话每分钟各查一次还会碰 GitHub 的速率限制。除非你另外决定，不做。

### C 不查，交给 Agent

- 只放一个「问 Agent：PR 和 CI 怎么样了」按钮，往终端里打一句话。不新增联网，但信息不进 Fibo，看板上看不到。

## 推荐

A。分步：

1. core 解析 + host 请求：`gh` 的 JSON → `PrStatus { number, url, state, draft, checks: { passed, failed, pending }, review }`，解析是纯函数，单测用录好的 `gh` 输出；host 处理 `PrStatus`，找 `gh` 的办法先验证（见风险）。
2. 窗口：设置开关、看板卡片、侧栏悬停提示、README FAQ 那一段改写。
3. 之后再议：CI 失败时把失败的检查名作为一句话发给该会话；「开 PR」交给 Agent（往终端打「开个 PR」），Fibo 不自己调 `gh pr create`。

## 接口和改动的文件（预估）

- `crates/fibo-core/src/protocol/request.rs`；新文件 `crates/fibo-core/src/pr_status.rs`（解析）。
- `crates/fibo-host/src/host/client/` 加处理 `PrStatus` 的文件。
- 窗口：看板卡片（`workbench/board/card.rs`）、侧栏会话行、设置页；界面文字走 `t!`，英文进 `crates/fibo-core/locales/en/`。
- README 和 README_CN 的 FAQ：写明「打开开关后，你看的时候才查 PR 状态」。

## 风险

- 找 `gh`：host 没有用户的 PATH。可选做法：用登录 shell 解析一次 `command -v gh`；或者只认几个常见位置（`/opt/homebrew/bin`、`/usr/local/bin`、`/usr/bin`）。动手前先在 GUI 启动的 App 里实测。
- `gh` 版本差异：`--json` 字段在旧版本里可能不全，缺字段时按「无」处理。

## 验收标准

- 开关关着时，日志里没有任何 `gh` 调用。
- 有 PR 的分支：卡片显示 #号和检查汇总；没装 `gh`、没登录、断网时什么都不显示，也不报错。
- 另一台电脑上的会话：用那台的 `gh` 查。
- 每帧的路径里不碰 `gh`，perf-gate 不退化。

## 不做

- 后台轮询（除非另开决定）。
- Fibo 自己创建或合并 PR。
- GitLab（Emdash 有）：先只做 GitHub。

## 要你拍板的问题

1. 开关默认关（推荐）还是开？
2. 除了看板卡片和侧栏悬停，要不要也放进窗格标题栏？
3. 「开 PR」交给 Agent（推荐），还是 Fibo 自己调 `gh pr create`？
