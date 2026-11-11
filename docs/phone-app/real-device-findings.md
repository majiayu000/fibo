# 只有真实环境才发现的问题

单元测试、假 Agent、网页版和模拟器都过了，放到真手机、真 CLI、真数据上才出问题。这里记下每一条：为什么之前看不到，现在由什么守着。

**要支撑的决定：** 手机功能什么时候才算做完，以及下一轮该先测什么。

**规则：** 改了手机能看到或能操作的东西，要在真手机上走一遍再说做完（做法见下面）。走不了就在汇报里写「没验：真手机」。发现新问题在表里加一行，能写测试的同时写测试。

## 四类问题

| 类型 | 为什么测试看不到 | 下次先查什么 |
|---|---|---|
| **时序和进程寿命** | 测试里所有进程同时启动、用同一份配置 | 换 App、更新插件、断网重连之后，每个长期进程手里的东西还对不对 |
| **平台差异** | 测试和网页版不跑安卓的原生层 | 用了 transform、手势、键盘、权限弹窗的地方 |
| **真 CLI 的行为** | 假 Agent 是我们按自己的理解写的，理解错了测试照样过 | Ctrl-C、Esc、弹框、后台命令在真 CLI 上到底发生什么 |
| **真实的量和时长** | 测试数据少、跑得短 | 几十张卡片、几百 MB 日志、挂一小时的流量和连接数 |

## 记录

| 日期 | 现象 | 类型 | 为什么之前看不到 | 修复 | 现在由什么守着 |
|---|---|---|---|---|---|
| 11-10 | 跑着时发的排队消息插进来后，那一轮的回复没有「干了 N 秒 · 跑了 1 个命令」，ping 那次调用在对话里看不到（11-09 已见） | 真 CLI | 真 Claude 在一轮中途把排队消息记成用户消息，夹在调用和回复之间；测试里的轮次都是「你说 → 调用 → 回复」 | 本分支：一段工作从 Agent 的上一条消息算起，不再从任何人的上一条算起；看它做了什么的列表同样改 | `log_index/chat/queued_tests.rs`；撤回后 (50, 0, 0) |
| 11-10 | 计划卡片里是原始 markdown，`#`、`##` 原样显示（11-09 已见） | 真 CLI | 测试里的计划只有一行标题和编号步骤，没看手机上怎么显示 | 本分支：电脑端拿掉标题的 `#` 和加粗的 `**` | `content/tests/turn_tests.rs` 计划那段；撤回后失败 |
| 11-10 | 断网时从深链重开对话，页面是「这段对话还没有消息」，输入框下写「尚未识别对话，暂时不能发送」；写的话留在输入框里，不进「重连后发送」。停在对话里再断网就正常排队 | 时序和进程寿命 | 清单的断网项一直是先进对话再断网；对话内容要连上电脑才拿得到，断网时新开的页面认不出是哪段对话 | 本分支：没连上时用卡片记着的对话 id 排进「重连后发送」，空页写「电脑没连上，连上后这里显示这段对话」。断网时旧消息仍看不到，要连上才读得到 | `mobile/test/sent.test.ts`「written while the computer is away, a message still knows its conversation」；真机没重走 |
| 11-10 | 从手机发图，发出后自己的气泡里没有缩略图，只在文字末尾多一段「[Image #1]」 | 真 CLI | 真 Claude 把图片写成记录里的 `[Image #N]` 占位，手机照原文显示；假 Agent 不产生这个占位 | 本分支：气泡把标记换成开头的「[1 张图片]」，和发送中那条一致。缩略图仍没有：记录里只有占位，图在电脑上 | `mobile/test/sent.test.ts`「a logged message shows its pictures the way it read while sending」；真机没重走 |
| 11-09 | 「在跑」区块切到终端，最后 8 行全是 Claude 的输入框、Tip 和底栏，看不到它在做什么 | 真 CLI | 测试里的屏幕是自己造的几行字，没有真 Claude 那样常驻在底部的输入框和状态栏 | #262：去掉最后两条横线之间的输入框和它下面的部分再取 | `mobile/test/tools.test.ts`「leaves out the input box and the footer under it」；11-09 OPPO PMA110 上重测看到 Bash(ping) Running… |
| 11-09 | 「终端最后几行」里 Claude 的 `⏵⏵` 显示成方框，`⏺` 变成橙色图标 | 平台 | 安卓系统字体没有 `⏵`，`⏺` 被当成 emoji 画；电脑和网页的等宽字体都有这两个字。第一次全量走查时看到的 | 未修 | 无自动测试；11-09 OPPO PMA110 全量走查 `chat.terminal` 的截图 |
| 11-09 | 记录被改短后，手机按 #186 整份换掉对话，但改写前那一轮的「干了 7 秒 · 跑了 1 个命令」没了；后台给的命令数从 1 变成 0 | 量 | 测试日志都小于 128 KiB。后台发现文件变短后只重读最后 128 KiB（和首次建索引一样），更早的工具调用要等「读回」；这次 491 KB 的记录里 echo 在第 286,783 字节。先清空再写的截法是从空文件一路追加读全的，看不出来 | 未修。真 Claude 不截短自己的记录；会改写历史的 CLI 上，改写前的回合摘要可能少算，翻到顶读回后才对 | 无自动测试；11-09 OPPO PMA110 上用 `os.truncate` 截短测试会话复现，`toolcalls` 返回 `calls: []` |
| 11-08 | 键盘挡住「新任务」的开始键、「记一件待办」的记下键和底部弹出的输入框（要求修改、新建文件夹）；聊天页输入框和键盘之间多空一截 | 平台 | RN 0.88 安卓全屏绘制，窗口不再为键盘缩小；只有聊天页包了 `KeyboardAvoidingView`，弹框只在 iOS 生效。网页版和 iOS 看不到。用户在真手机（搜狗输入法）上报的 | 所有带输入框的页面统一包 `KeyboardSafe`（`mobile/src/ui/keyboard.tsx`），键盘弹起时不再留系统栏的空 | 无自动测试，靠真机看：11-08 OPPO PMA110（安卓 16）＋搜狗九宫格，旧包在「新任务」复现（开始键整个不见），新包修好；同一台上旧包的聊天页没被挡，搜狗拼音候选出来也没挡 |
| 11-08 | Claude 用提问框问三选一，手机只写「等你回答」，点进去只有「CLI 请求工具审批（Hook）」和 Esc，会话卡住 | 真 CLI | 这一版 Claude Code 答完才把 AskUserQuestion 写进日志，答之前只有 PermissionRequest hook；测试里的提问都从日志读 | `b7b9b93`：读 hook 的 tool_input，给手机编号选项 | `turn_tests::a_question_asked_through_the_permission_hook_keeps_its_question_and_choices` |
| 11-08 | Claude 计划模式问「按这个计划开始做吗」，手机只显示「CLI 请求工具审批（Hook）」；计划里的「1. 2.」步骤还会被手机当成选项按钮 | 真 CLI | ExitPlanMode 也走 PermissionRequest，没人读它的 tool_input；菜单文字随版本和模式变 | 读出计划进卡片，步骤号改成「1、」，不给按钮，手机用看终端加 ↑ ↓ 回车（真 Claude 2.1.319 从手机批准成功） | `turn_tests::a_question_asked_through_the_permission_hook_keeps_its_question_and_choices` |
| 11-08 | 多选题按数字只是勾选，还要移到 Submit、再确认一次「Submit answers」 | 真 CLI | 只按单选题设计过数字键 | 多题或多选不给按钮，手机用 ↑ ↓ 回车（真 Claude 上从手机走完整个多选） | `content::tests::a_question_tool_offers_buttons_only_for_one_single_choice_question` |
| 11-08 | Codex 启动先弹「Update available」菜单，此时还没有对话，手机上只是一个 shell，没有路走 | 真 CLI | 菜单不写日志、不发 hook，测试里的 Codex 没有更新可用 | Agent 起来但还没对话时，对话页给看终端和 ↑ ↓ 回车 Esc（真 Codex 0.166 从手机选了 Skip） | 无自动测试，靠真机看 |
| 11-08 | Claude 用提问框问三选一，手机只写「等你回答」，点进去只有「CLI 请求工具审批（Hook）」和 Esc，会话卡住 | 真 CLI | 这一版 Claude Code 答完才把 AskUserQuestion 写进日志，答之前只有 PermissionRequest hook；测试里的提问都从日志读 | `b7b9b93`：读 hook 的 tool_input，给手机编号选项（真 Claude 2.1.319 上从手机点「蓝色」，数字键直接答上） | `turn_tests::a_question_asked_through_the_permission_hook_keeps_its_question_and_choices` |
| 11-07 | OpenCode 有时不认：权限框弹着，手机没有卡片。提示词短能认，长到折行就认不出，看着随机 | 时序 | 换 App 不更新插件，机器上还是 0.2.1，提示词时间全是 0，「按回车的时间」这条证据失效，只剩屏幕原文，而 `┃` 把折行的提示词隔开。仓库和测试都是 0.2.2 | 更新插件到 0.2.2 后同样的长提示词能认；`mooring plugins` 看是否「可更新」。换 App 后不自动更新插件这点没改 | 无自动测试，靠真机看 |
| 11-07 | `mooring run opencode` 必定超时「一直没有出现输入框」，任务没交 | 真 CLI | 就绪判断只认 `❯ › >`，OpenCode 的框没有箭头；插件测试没走 `run` | 插件声明 `input_hint` | `tests/opencode_plugin.rs` 的 `its_empty_input_box_counts_as_ready_and_its_output_does_not` |
| 11-07 | 手机点「允许」多弹一个框，写「电脑没读出它要做什么」，其实读出来了 | 时序 | 手机连的 `mooring remote` 比插件更新早启动 7 秒，还记着旧插件。协议测试每次都起新进程，碰不到 | #165：只看卡片上的字段，不查本进程的注册表 | `remote::asks::api_tests::an_approval_from_the_api_is_read_off_the_card` |
| 11-07 | OpenCode 被中断或拒绝权限后一直「在跑」；Mooring 的中断发 Ctrl-C，会把它整个退出 | 真 CLI | 只读日志时看不到；中断和拒绝不写 `step-finish`，Ctrl-C 的后果要真按才知道 | #134：插件声明中断按键，SQL 补报错和中断 | `tests/opencode_plugin.rs` |
| 11-07 | 开了本地端口后，跨站请求能在 OpenCode 里建会话 | 真 CLI | 接口本身是好的，这是把它打开后才有的风险，要真发一个跨站请求才看得到 | #148：每次启动随机密码，无密码 401 | `cli_http::tests::credentials_go_in_the_header`（只守请求头；401 是当时实测的） |
| 11-06 | 正在跑后台脚本的 Claude，换 App 后变「等后台」 | 时序 | 宽限期 2 分钟内都对；要等够时间、而且不再有 hook 才会错 | `12f4ce8`：启动时按进程号找回 | `restart_binding::a_new_host_finds_the_process_of_a_quiet_plain_claude` |
| 11-06 | 收件箱「11 件等你」要翻两屏才找到；每行开头都是「全部放行」，等于没说 | 量 | 演示数据只有几张卡片，全部放行也不是常态 | `4459fca` | `mobile/test/inbox.test.ts`、`modes.test.ts` |
| 11-06 | 手机挂着每小时 54–76 MB 流量 | 量 | 只有真挂着、按 uid 读流量才看得到；一份列表被拆成几十条消息，每条事件又整份重发 | `70412e9`：只发变化的卡片。真机 0.7 MB/时（当时的事件率） | 进度页记的真机读数 |
| 11-06 | 279 MB 的日志，手机上只看到 2 条消息 | 量 | 测试日志很小；第一次只读尾部 128 KiB | `892fd4a`：翻到头时读回 | 见 `v2-plan.md` |
| 11-05 | 安卓上「这个对话没有消息」左右镜像 | 平台 | 倒序列表的翻转样式在安卓是上下左右都翻，iOS 和网页只翻上下 | `9ddcadf` | 无自动测试，靠真机看 |
| 11-05 | 同一部手机最多同时 20 条连接 | 时序 | 显示看着正常，翻 `remote-log.jsonl` 才看到成批连接；拨号途中再连会多拨 | `ef31787` | 见提交说明 |
| 11-05 | 审批卡片写错命令：真 Claude Code 的 Tip 行顶替了命令；`git push` 的二次确认也丢了 | 真 CLI | 假 Agent 画的画面里没有 Tip 行 | `94ed0cc`：解析挪到电脑上，加 `ask` | 见 `v2-plan.md`「真机实测查出的问题」 |
| 11-05 | 手机「停下」对 Codex 没真停；Codex 空闲时收到 Ctrl-C 会退出 | 真 CLI | 只有真 Codex 才这样，Esc 后命令还接着跑 | `38e4c5c` | `v2-plan.md` 的真实 CLI 行为表 |
| 11-11 | 英文界面「我的」页额度写成「Codex This week 97%」，窗口名在句中却大写开头 | 真实的量和时长 | 网页版样例数据没有额度读数，这一行在网页版上根本不出现 | 句中的窗口名首字母改小写（`shortWindow` 挪进 `ui/parse.ts`） | `test/i18n.test.ts`「a quota window named mid-line…」 |

11-05 之前的实测详情见 [v2-plan.md](v2-plan.md) 的「真机实测查出的问题」和「真机实测后留下的四件事」。

## 卡片说等审批，屏幕上却没有

屏幕一变就没了，事后看不出当时读到了什么。运行目录的 `screen-waits.jsonl` 记着每次判成等待的时间、终端、命中的规则、问题和屏幕底部 16 行，以及它何时结束：

```sh
tail -20 ~/.local/share/keepline-workbench/screen-waits.jsonl
```

11-07 的一张卡片是这样：bypass 模式、记录里没有审批、也没有子 Agent，却写着「Do you want to proceed?」。怀疑是运行中命令的输出被当成了审批框，未证实。到 11-21 没再出现就关掉这件事。

## 怎么走真机（安卓）

每次都走 [walkthrough.md](walkthrough.md) 的整张清单：用 `scripts/phone-walk.py` 截图、记结果、出报告。下面是底层的 adb 命令。

手机用 USB 连着电脑，在 App 里已经配对。`adb` 在 `~/Library/Android/sdk/platform-tools/adb`，不在 PATH 里。

```sh
A=~/Library/Android/sdk/platform-tools/adb
$A devices -l                                          # 看连着没有
$A shell dumpsys window | grep -E 'mCurrentFocus|isKeyguardShowing'   # 解锁了吗，前台是谁
$A exec-out screencap -p > shot.png                    # 看屏幕
$A shell uiautomator dump /sdcard/w.xml && $A exec-out cat /sdcard/w.xml   # 取元素文字和坐标
$A shell input tap <x> <y>                             # 点
```

React Native 的文字在 `uiautomator` 里读得到，按文字找 `bounds` 取中心点，比从截图估坐标准。

顺序：

1. 在真实后台上开一个会话，目录用临时目录，让它做无害的事（`echo`），这样卡片内容一眼能认出是测试。
2. 从手机首页点进去，每一步看截图，也用 `mooring list`、`mooring read` 对照电脑上的结果。
3. **把版本交接也走一遍：** 换 App、更新插件之后马上再点一次。11-07 那个问题只有这样才看得到。
4. 收尾：关掉测试终端，删掉测试会话和临时目录。

约束：

- 只操作 Mooring，只点自己开的测试项。真实会话的按钮不能点。
- 手机截图可能有私人内容，放临时目录，不进仓库。要进仓库的另外裁剪、确认。
- 「以后都允许」这类会留下放行规则的按钮，除非正好在测它，否则不点；点了要查清楚写到了哪里。

iOS 真机还没走过完整流程。
