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
