# 手机端第二版：照设计稿「新外观」全做（2026-11-04）

用户 11-04 说：「手机要都做啊，按照之前的 UI 和功能」。依据是 [design.html](design.html) 里标「推荐」的「新外观 · 参照 T3 Code」那一节，以及它下面的「处理一件事」「选择目录」「电脑、额度与设置」三节。

## 要支撑的决定

离开电脑时，手机能不能把一件事从头处理完：看到要处理的事、看懂、批准或回复、看改动、开新任务、查额度。全靠手机 App，不用回到电脑前。

验收时在真手机上走一遍「收件箱 → 审批 → 会话 → 改动 → 新任务」。每一屏都要和设计稿截成相同比例并排对照。

## 放弃条件

- 某一屏的数据电脑端拿不到，而且一天内做不出接口：这一屏先只做外观，标「电脑端还没接」，不编造数据。
- 推送通知需要 APNs 或 FCM 账号，账号由用户提供。这一轮不做推送，设置页的开关只存在本机，并写明「推送还没接」。

## 屏和数据

| 屏 | 数据（现有 / 新增） |
|---|---|
| 收件箱（首页，平铺的行，审批行内批，左滑已处理、右滑稍后，断网时写的话放进「未发送」） | list、events、answer、outcome（已处理即 done，稍后即 deferred） |
| 会话（Agent 回复不加气泡，工具过程折成一行，输入卡片，停下 / 继续，一键回复，「改动」和「终端」胶囊） | chat（**新增 `tools: true`**）、send、keys、read、**stop** |
| 改动（每个文件一张卡片，可勾「看过」，底部有汇总，「要求修改」和「让它提交」两个按钮） | **diff** |
| 审批详情（一句人话说明、命令用大号等宽字、Face ID、「会话里都允许」） | answer（**新增 `always`**）、read、chat |
| 新任务（选电脑、目录、Agent，写一句话） | open（**新增 `agent`、`prompt`**） |
| 选择目录（逐层浏览、搜索、新建） | **dirs**、**mkdir** |
| 看板（按 phase 和 status 分列，「今天完成」看 outcome=done 且今天有更新） | list |
| 电脑（在线状态、连接方式、手机自己测的延迟、权限、系统） | hello（**新增 `os`**）、ping |
| 额度 | **quota** |
| 通知与设置（只存在本机：外观、打开时要 Face ID、危险命令多确认一次） | 无 |
| 配对（App 内扫码，粘贴链接作备用） | 现有 |

## 新增的协议（`crates/mooring-remote/src/protocol.rs`，hello 的 `ops` 会列出）

所有写操作都带 `request`，规则和第 4 阶段一样。权限检查只在电脑端做；dirs 会暴露电脑上的目录结构，所以只给 full。

| op | 请求 | 回复 `ok` | 权限 |
|---|---|---|---|
| `diff` | `{terminal}`，或 `{cwd}` | `{cwd, branch, files:[{path, added, removed, status:"M"/"A"/"D"/"R"/"?", patch, truncated}], added, removed, truncated}`。只读工作区的 `git diff HEAD` 和未跟踪文件；跑 git 时带 `-c core.fsmonitor=false`；单个文件 patch 最多 64 KiB，总共最多 512 KiB；不是 git 仓库时报错「这里不是 git 仓库」 | 三档都能用 |
| `dirs` | `{path?}`（省略时是家目录，`~` 会展开）或 `{query}` | `{path, parent, home, dirs:[{name, path, git?:{branch, changes, worktree}, sessions, recent}], recent:[path…], truncated}`。只列文件夹，跳过隐藏文件夹、`node_modules`、`.git`、`target`；`query` 在家目录下按名字找，深度不超过 6、结果不超过 50 条，最多走约 1 秒，没走完时 `truncated:true`（手机写「没搜完」），最近用过的和 git 仓库排在前面 | full |
| `mkdir` | `{path, request}` | `{path}`；父目录必须已经存在 | full |
| `open` | 现有字段加上 `agent?:"claude"/"codex"` 和 `prompt?`。带 agent 时，在新终端里运行这个 Agent，并把 prompt 作为第一句话交给它 | 不变，加一个 `terminal` | full |
| `answer` | 现有字段加上 `always?:bool`。为 true 时选终端菜单里「本会话不再询问」那一项；画面上没有这一项就报错，不退回到普通允许 | 不变 | approve 和 full |
| `stop` | `{terminal, request}`。电脑先确认画面上 Agent 正在跑，再中断它，效果和桌面端的停止一样；没在跑就报错 | `{}` | full |
| `chat` | 现有字段加上 `tools?:bool`。为 true 时，每条 Agent 消息带 `tools:{commands:N, files:[{path, added, removed}]}`，统计的是它和上一条用户消息之间的工具调用 | 不变 | 三档 |
| `quota` | `{}` | `{agents:[{agent, plan, at, windows:[{minutes, used, resets_at}]}]}`，直接取 `mooring_core::quota`，读数不超过 7 天 | 三档 |
| `hello` | 不变 | 回复加上 `os:"macos"/"linux"` 和 `version` | — |

「终端最后几行」继续用 `read`。延迟由手机自己量：从发出 `ping` 到收到回复的时间。

## 不做

- 推送（原因见上）。
- 看板里的「待办」一列：桌面端的待办还没进远程协议，这一列写「待办只在电脑上」。
- 审批的失效时间、按命令类型写后果说明（例如「推送 6 个提交」）：电脑端拿不到可靠的数据，这一轮不做，只给出命令全文和它上一句话。
- 传图片。

## 已定的外观细节

- 「在跑」用黄色，和桌面端一致（见 status::of 的规则）。浅色背景上用深一档的琥珀色，保证对比度。
- 看板、电脑、额度收进收件箱的标题栏，不做底部标签栏，按新外观来。

## 结果

2026-11-05 电脑端协议做完（分支 `feat/phone-v2-host`）。测试在 `crates/mooring-host/tests/remote_v2.rs`、`remote_v2_agents.rs`，全部经 `mooring remote --stdio` 走。协议文档还在旧仓库 keepline，这里以 `crates/mooring-remote/src/protocol.rs` 的注释为准。

上表都已实现，下面这些和上表不完全一样，等手机端确认：

- `hello` 的 Mooring 版本叫 `app_version`：`version` 已经是协议版本（数字 1），不能同名。
- `chat` 的 `tools` 统计的是这条 Agent 消息和**上一条消息**（用户或 Agent）之间的工具调用，不是和上一条用户消息之间。一轮里 Agent 说了几句时，按上表的写法同一个调用会被算几次。失败的编辑不算改了文件，失败的命令算跑过。Codex 的命令来自日志里每条命令的执行记录，文件来自 `exec` 脚本里写成字符串的 `apply_patch`；运行时才拼出来的 patch 读不到。
- `dirs` 的 `git.changes` 可能没有：单次列目录里数改动最多花 3 秒，来不及的不写，免得看起来像没有改动。
- `dirs` 的 `query` 还跳过家目录下的 `Library`，否则 1 秒都耗在那里。
- `stop` 发 Ctrl-C，也就是桌面端的「中断（Ctrl-C）」，Agent 还在，可以接着说；不是「停止 Agent」（那个会结束进程，手机上的「继续」就没法用了）。
- `answer` 的 `always` 选菜单里的「不再询问」一类选项：Codex 的 `(a)`，Claude Code 的 `don't ask again for …` 和 `switch to accept edits`。**Claude Code 这几项的范围不一定是本会话**，要看它自己怎么存。
- `open` 的 `agent` 认注册过的所有 CLI，不只 claude 和 codex。
- `mkdir` 时文件夹已经在也算成功，所以重发安全。
- `diff` 的回复保证不超过一行上限（1 MiB）：内容转成 JSON 后变长时，从后往前丢 patch，计数保留。未跟踪的符号链接只列出，不读目标。
- 新增 `mooring remote --stdio --role view|answer|full`，只能收窄权限，测试和只读的 ssh 连接用。
## 结果（2026-11-05，分支 feat/phone-v2-ui）

**决定：界面和协议接线采用；「整件事能不能只靠手机处理完」待验证**，要等真手机走一遍「收件箱 → 审批 → 会话 → 改动 → 新任务」。

做了：计划表里的全部屏（收件箱、会话、改动、审批详情、新任务、选择目录、看板、电脑、额度、设置、配对）。对照图在 [v2-compare/](v2-compare/)：`<屏>-<主题>.png` 左边是设计稿，右边是 App；`other-theme-*.png` 是另一种主题的同几屏；`ios-simulator-real-host.png` 是 iOS 模拟器连真后台。

对照图里的数据来自截图桩（`src/fixture.ts`，只在 `EXPO_PUBLIC_FIXTURE=1` 时加载，正式包里查过没有这段内容），内容照设计稿写，只用来比外观。接线是另外连真后台验的。

已验（真后台：`feat/phone-v2-host` 的 `phone_demo_machine`，真中继，看了截图）：

- 网页版（无头浏览器 390×844）：配对先确认再保存；审批行显示抠出来的命令，「允许」先弹二次确认，之后终端打出 `pushed`；「以后不再询问」在菜单没有这一项时显示电脑的报错；发消息后收到回复；断网时写的话进「未发送」，恢复后自动发出并收到回复；触摸左滑进「已处理」，在已处理里右滑放回；`diff` 读到真仓库，勾「看过」；`dirs` 浏览和搜索、`mkdir` 真的建了文件夹；`open` 真的起了 Claude Code 并回了话；`quota` 读到真读数；`stop` 发出后演示 Agent 退出（演示 Agent 是个 shell 脚本，Ctrl-C 会杀掉它，真 Agent 只中断这一轮）。
- iOS 模拟器（iPhone 17 Pro，Debug 包）：深链配对、收件箱、审批详情、原生二次确认弹窗、`pushed`、左滑已处理、发消息收到回复、新任务、选择目录；图标、渐变、深色都正常。

没验：真手机；Android 这一版没编译运行；Face ID / 指纹真的弹出来（网页没有，模拟器没走）；相机扫码；软键盘弹出时输入卡片的位置；真 Claude 会话里的工具统计和 `always` 成功的情况；真 Codex；公网中继；「中继太旧」只有单测（靠关闭帧里的文字判断，电脑实际是自己停下，手机不一定收得到这句话）。

和设计稿有意不同的地方：

- 在跑是黄色（已定）；深色下「等你」因此从黄色改用桌面端的橙色，不然两个状态分不开。
- 标题栏放四个图标加新任务，设计稿只有两个；副标题和行的第三行写目录，因为 `list` 没有分支。
- 没有这些数据就不显示：运行时长、「干了 2 分钟」、权限模式和模型标签、附件按钮、审批失效时间、后果说明、测试结果、今天 tokens 和金额。
- 看板、电脑、额度、设置等设计稿还是旧外观的屏：去掉底部标签栏，大标题改成返回键加标题。
- 文字用系统字体（SF / PingFang），设计稿用 DM Sans 和 JetBrains Mono。

还差电脑端：`outcome` 还不接受 `deferred`，所以右滑「稍后提醒」现在只会提示「还不支持」。真后台上用 `open` 起的 Agent，在演示环境里卡片一直是普通终端（没装 hook），手机上就看不到对话，要在装了 hook 的真环境确认。

## 合并时改的（2026-11-05）

- `outcome` 接受 `deferred`：手机右滑「稍后提醒」，一小时后自动回到收件箱。
- 全量测试里的 `guard` 查出手机自己写状态词：去掉 `statusWord` 的旧电脑兜底，状态词一律用卡片的 `status_label`。
- 看板和桌面的 `remote_column` 用同一套分列，列名也一致：要你处理、在跑、停下了、待办、做完了、很久没动。设计稿的「待核对」「今天完成」是 11-03 之前的说法。
- 设置里的通知项「做完了」改成「等你验收」：一轮跑完不算做完，做完由用户决定。

## 额度和多台电脑（2026-11-05，用户定）

- 额度只从配对的电脑来。手机唯一的网络出口是连中继的 WebSocket，中继只转发加密内容，手机不直接找任何服务商。
- 同一页按电脑列出，不相加：两台电脑可能共用账号，也可能不是，电脑只报套餐、不报账号。
- 电脑离线时显示这台手机上次收到的读数，标「断开前」；重新连上就换回实时读数。取消配对时一起删掉。

## 补齐和设计稿的缺口（2026-11-05，分支 feat/phone-gaps2）

**决定：分支名、在跑时长、干了多久、接着上次的对话、设为空间、权限和模型标签、今天 tokens、看板待办、改动页测试结果，全部采用；金额否定；「真手机上好不好用」待验证。**

新增的协议（hello 的 `ops` 会列出，老电脑不列，手机就不显示入口）：

| op / 字段 | 内容 | 权限 |
|---|---|---|
| `list` 的卡片 | `branch`、`running_since`、`doing`、`model`、`permission` | 三档 |
| `list` | `todos:[{id, space, title, agent}]`，和桌面看板的「待办」同一个函数（`status::open_todos`） | 三档 |
| `chat` 的 `tools` | 加 `took`（秒，不知道时不写） | 三档 |
| `diff`（带 `terminal`） | 加 `checks:[{command, counts, ok, at, stale}]` | 三档 |
| `open` | 加 `resume`（会话 id）。在这段对话自己的目录里跑 `Workspace::resume_command`（和桌面同一个）；用户的话走 `$1`，不进脚本；还在跑就回 `{terminal, existing:true}`，不开第二份；目录不在了就报错，不换目录 | full |
| `lastsession` | `{cwd, agent}` → `{session:{id, title, updated, terminal?}}`，只看桌面会列出的对话 | full |
| `addspace` | `{path, request}` → `{id, name, cwd}`；已经是空间就返回原来的，重发安全 | full |
| `usage` | `{since}` → `{since, agents:[{agent, tokens, cached}], partial}`；手机传自己的零点；不占请求队列，一次最多读 4 秒，没读完 `partial:true`，手机接着问 | 三档 |

### B 项的证据

**权限模式和模型：可靠，已显示（只读）。**
- Claude：模型在 assistant 行的 `message.model`（真实日志里 26 325 行都有，`<synthetic>` 要跳过）；权限在 `permission-mode` 行和用户提问行的 `permissionMode`（今天的日志里 8 550 处）。
- Codex：每轮的 `turn_context` 有 `model`、`approval_policy`、`sandbox_policy.type`。
- 真实日志跑过解析器：Codex 得到 `gpt-6.1-sol`、全部放行 / 只读，Claude 得到 `claude-opus-5-5`、全部放行 / 逐个确认。
- 限制：显示的是日志最后写的值。真实日志里模式切换落在下一条提问行或下一次状态行上，所以 Shift+Tab 之后、下一次写日志之前可能还是旧值。升级前已索引的对话，要等写新日志才有这两项，没有就不显示。

**今天 tokens：可靠，已显示；金额不显示。**
- crates 里原来没有本地计量（搜不到 `input_tokens`），新写在 `agents/usage_lines.rs`。
- Claude 一条回复按内容块写多行，每行带同一份 `usage`：今天的日志里重复行 14 446、实际回复 11 975，不去重会多算一倍多，所以按 `message.id` 去重。Codex 8 个文件逐个核对过，`last_token_usage` 之和等于最后的 `total_token_usage`。
- 拿真实日志核对：`usage` 回 Claude 8 244 434 / 缓存 284 439 629、Codex 505 766 / 9 148 928，和另写的 Python 统计逐位相同。缓存读取比其余大两个量级，所以单独写「另有缓存读取」，不算进大数字。
- 金额：crates 里没有价格表，模型和缓存价又常变，订阅用户也不按 token 付钱，算出来的数不可靠。
- 限制：只算这台电脑本地日志里的 Claude 和 Codex。同一天 fork 的 Codex 会话如果复制了父会话的计数，可能重复（没遇到，没专门测）。

**待办：可靠，已接上。** 数据就在 Workspace，`list` 带上，手机看板只读显示。判定挪进 core，桌面和手机同一个函数。

**改动页测试结果：可靠，已显示。** 来源是日志里测试命令的退出码（`Evidence` 的 `test`，桌面验收也用它），不看 Agent 怎么说；带管道的命令本来就不算。`stale` 表示这段对话之后又改过文件，手机写「通过，之后又改过」。限制：只知道这段对话自己的改动和测试，别的会话或手动改的不知道；只有 `diff {terminal}` 有。设计稿的「8 个测试通过」这种汉化计数没做，显示命令加通过 / 没通过。

### 和设计稿还不一样的地方

- 一键回复「打包并安装 / 先别」：那是从 Agent 的自由文字里猜的，不做。编号选项和问题工具的选项照旧。
- 工具摘要点开「看每条命令」：现在有改动时打开改动页，没做逐条列表。
- 附件按钮、审批失效时间、后果说明：不做（见上文）。
- 选目录底部「上一级」换成了「设为空间」，和设计稿一致；上一级用顶部路径点回去。
- 只配了一台电脑时，会话标题栏不写电脑名，给分支和状态词留位置；配了多台才写。

### 已验 / 没验

已验：全量 `cargo test`（core、remote、host、relay）569 个通过，clippy 和 fmt 过；`mooring remote --stdio` 测了每个新字段和 op，含权限和重发；网页版 390×844 浅深两色连真后台看了每个新元素。恢复时终端打出 `fake claude: --resume <id> 接着把 $(touch /tmp/pwned-gaps) 'x' 做完`，文件没有被创建。`list` 在 8 张卡片时中位 1.4 ms（debug 包，经 stdio）。

没验：真手机；真 Claude Code 和真 Codex 跑 `open resume`（`--resume id prompt` 只对过两个 CLI 的 `--help`，假 CLI 上跑通）；iOS 和 Android 原生的新界面；真后台上 Codex 卡片的标签（解析器用真实日志验过，界面没有）；Shift+Tab 之后标签多久变（只看过日志顺序）。

### 怎么复现

`cargo test -p mooring-host --test phone_gaps_demo -- --ignored --exact phone_gaps_demo --nocapture` 起演示电脑（假 Agent、真后台、真中继、独立的 HOME 和真 git 仓库），`cd mobile && bunx expo start --web`，粘贴打印的链接。对照图是 [v2-compare/](v2-compare/) 的 `gaps-*.png`，左设计稿右 App。测试在 `crates/mooring-host/tests/remote_v2_gaps.rs`。
