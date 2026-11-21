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
| 11-11 | Claude 2.1.322 首次 ExitPlanMode 已在电脑显示两步计划和确认选项，手机却只显示「CLI 请求工具审批（Hook）」，没有计划正文；取消并重新调用后出现计划正文。可点「看终端」走退路 | 真 CLI、版本 | 11-08 有过同样表象的修复记录；本轮真 CLI 首次审批再次复现，不能只用带 plan 字段的模拟输入或重试成功证明当前 CLI 可用。本次原始日志确认首次 Write 与 ExitPlanMode 并行，后者参数为空 | 本分支：从 CLI 日志自己的 slug 定位计划文件；Write 结果和审批 Hook 刷新正文，批准结果没有退出码也能离开等待。空参数、文件稍后落盘及重启有回归测试；新宿主真机已验并行 Write 后显示最新正文及箭头回车批准 | 本地私有证据 `~/Documents/claude-pages/mooring/2026-11-11-phone-walk-2/card.plan-1.png`、`card.plan-2.png`、`chat.terminal-3.png`、`plan-before.txt` 与 `keys-*.txt` |
| 11-10 | 跑着时发的排队消息插进来后，那一轮的回复没有「干了 N 秒 · 跑了 1 个命令」，ping 那次调用在对话里看不到（11-09 已见） | 真 CLI | 真 Claude 在一轮中途把排队消息记成用户消息，夹在调用和回复之间；测试里的轮次都是「你说 → 调用 → 回复」 | #281：一段工作从 Agent 的上一条消息算起，不再从任何人的上一条算起；看它做了什么的列表同样改 | `log_index/chat/queued_tests.rs`；撤回后 (50, 0, 0)；11-12 真机走过 |
| 11-10 | 计划卡片里是原始 markdown，`#`、`##` 原样显示（11-09 已见） | 真 CLI | 测试里的计划只有一行标题和编号步骤，没看手机上怎么显示 | #281：电脑端拿掉标题的 `#` 和加粗的 `**` | `content/tests/turn_tests.rs` 计划那段；撤回后失败；11-12 真机走过 |
| 11-10 | 断网时从深链重开对话，页面是「这段对话还没有消息」，输入框下写「尚未识别对话，暂时不能发送」；写的话留在输入框里，不进「重连后发送」。停在对话里再断网就正常排队 | 时序和进程寿命 | 清单的断网项一直是先进对话再断网；对话内容要连上电脑才拿得到，断网时新开的页面认不出是哪段对话 | #281：没连上时用卡片记着的对话 id 排进「重连后发送」，空页写「电脑没连上，连上后这里显示这段对话」。断网时旧消息仍看不到，要连上才读得到 | `mobile/test/sent.test.ts`「written while the computer is away, a message still knows its conversation」；11-12 真机走过 |
| 11-10 | 从手机发图，发出后自己的气泡里没有缩略图，只在文字末尾多一段「[Image #1]」 | 真 CLI | 真 Claude 把图片写成记录里的 `[Image #N]` 占位，手机照原文显示；假 Agent 不产生这个占位 | #281：气泡把标记换成开头的「[1 张图片]」，和发送中那条一致。缩略图仍没有：记录里只有占位，图在电脑上 | `mobile/test/sent.test.ts`「a logged message shows its pictures the way it read while sending」；11-12 真机走过 |
| 11-12 | 按「停下」或拒绝审批后，那一轮没有回复，它的调用和时长并进了下一条回复：diff-ready 那条写「干了 2 分钟 · 跑了 1 个命令」，实际约 20 秒、没跑命令，那 1 个命令是被停下的 ping（拒绝后同样：BLUE-REASON 那条算上了被拒的 touch） | 真 CLI | 真 Claude 中断时只记一行「[Request interrupted by user…]」，没有回复；#281 让一段工作从 Agent 的上一条消息算起，测试里中断之后没有下一轮。#281 之前这些调用哪条回复都不算 | #317：一段工作从 Agent 的上一条消息或之后最近一次中断算起，按页计数遇到中断清空；中断记录本来就在索引里，旧对话不用重建。被停下那轮的调用不再出现在任何回复的计数里 | `log_index/chat/queued_tests.rs`「a_stopped_turn_does_not_count_on_the_next_reply」，撤回后 (1, 120)；11-12 17:10 真机走过：停下后 diff-ready 那条「干了 14 秒 · 改了 notes.txt」，列表只有 Write、Edit；拒绝后 BLUE-REASON 那条不带被拒的命令 |
| 11-12 | 计划卡片里行内代码的反引号原样显示（`` `epsilon` ``） | 真 CLI | #281 只拿掉了标题的 `#` 和加粗的 `**`，测试里的计划没有行内代码 | #329：每行去掉成对的单个反引号，整行的 ``` 围栏不动；真机还没看过（电脑 App 是 #329 之前的 0acc31b） | `content/tests/turn_tests.rs` 计划那段；真机截图 `2026-11-12-phone-walk-3/card.plan-1.png` |
| 11-13 | 新任务带图片（#320）：Claude Code 那条气泡写「[1 张图片] …」，Codex 那条只有文字，没有「[1 张图片]」 | 真 CLI | Codex 把图记在消息外，日志里的用户消息只有文字；Claude 的 `@路径` 在消息里，手机能折成「[N 张图片]」。两边都读到了图（都答 WALK 4217） | 没修，只记下 | 无；真机截图 `2026-11-13-phone-walk/new.task-7.png`、`new.task-10.png` |
| 11-16 | 断网时排着的消息，重连后撞上打开的审批被挡，标成「没发出去（点一下重发）」；审批答完再点，永远还是「它在等你审批」，只能删掉重打 | 真数据 | 手机重发沿用原请求编号，后台 `once` 把第一次的拒绝记成这个编号的最终结果；原来的测试每次发送都是新请求，没有「同一编号再来一次」 | `db9130b0`（随 #366 合入）：写入前被挡的拒绝不留结果，同一编号重发重新执行。修复前已被挡的那条仍要删掉重打（结果留一天） | `remote_message_safety::approval_blocks_text_images_queue_steer_and_sendnow` 里同编号重发那段；真机截图 `2026-11-16-phone-walk-2/card.sendguard-*.png` |
| 11-16 | 两处会告诉你下一步怎么做的提示被省略号截掉：首页另一台电脑那行「…协议不一致，请把两端更新到同一版本」，对话里「没发出去：…（点一下重发，按住删除）」 | 真数据 | 原因文字比测试里的短句长，一行放不下；网页版窗口宽 | 没修，只记下 | 无；真机截图 `2026-11-16-phone-walk-2/home.list-1.png`、`card.sendguard-5.png` |
| 11-09 | 「在跑」区块切到终端，最后 8 行全是 Claude 的输入框、Tip 和底栏，看不到它在做什么 | 真 CLI | 测试里的屏幕是自己造的几行字，没有真 Claude 那样常驻在底部的输入框和状态栏 | #262：去掉最后两条横线之间的输入框和它下面的部分再取 | `mobile/test/tools.test.ts`「leaves out the input box and the footer under it」；11-09 OPPO PMA110 上重测看到 Bash(ping) Running… |
| 11-09 | 「终端最后几行」里 Claude 的 `⏵⏵` 显示成方框，`⏺` 变成橙色图标 | 平台 | 安卓系统字体没有 `⏵`，`⏺` 被当成 emoji 画；电脑和网页的等宽字体都有这两个字。第一次全量走查时看到的 | 代码已修（`36992dba`：换成手机字体有的 `▸` `●`），真机待复验 | 无自动测试；11-09 OPPO PMA110 全量走查 `chat.terminal` 的截图 |
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
| 11-20 | build 1479 的 Codex 会话里，手机声称排队消息已立刻发出。Codex 在上一轮结束后才读到它 | 真 CLI | Codex 的回车可以发送新文字，空输入框时按回车不会把队列送进当前步骤。宿主把两个能力报成了一个 | 分开声明两项能力，旧客户端的空输入立刻发送请求也会被拒绝 | build 1537，Android 真机两条 Codex 消息确实按顺序排队，气泡没有立刻发送入口。证据 `2026-11-20-phone-walk-4/chat.sendnow-1.png` |
| 11-20 | build 1537 的 Codex 忙时，空输入框仍写「现在发的会马上插进这一步」，按发送实际将两条消息排队，ping 结束后才按顺序进入对话 | 真 CLI、输入方式 | 输入框使用宿主报出的普通回车行为，Composer 在这个测试里实际选择了 queue；气泡已正确写「已排队」 | 输入框按 Composer 将实际发送的模式选提示。只有未指定排队或引导时，才用宿主报的普通回车行为 | Android 真机 `2026-11-20-phone-walk-4/chat.queue` 的前后截图和桌面的排队记录；`mobile/test/busy.test.ts` 覆盖 Codex 的组合，修后真机未验 |
| 11-20 | build 1479 的 Codex 会话里，手机和电脑均显示已中断，测试回合的 ping 子进程仍运行超过 30 秒 | 真 CLI | 真 Codex 的回合状态和子命令寿命分别变化 | 宿主等 Codex 重画后台终端提示再回报。后台仍在时明确提示，并保留会话中的后台服务。0.171.0 可以按后台进程 ID 停进程，但正在运行的命令查不到回合归属，自动结束本轮子命令仍待解决 | 本机走查 `2026-11-20-phone-walk` 的 `chat.stop`。真 Codex 0.171.0 单独复现过 Esc 后 sleep 继续运行。宿主回归 `remote_stop_reports_a_command_still_running_after_codex_logs_the_interrupt`，build 1537 手机真机复验，点停下后显示「回合已中断，Codex 仍有后台命令在运行」，同一测试 ping PID 仍在。本机证据 `2026-11-20-phone-walk-4/chat.stop` 和 `codex-stop-process.json` |
| 11-11 | 英文界面「我的」页额度写成「Codex This week 97%」，窗口名在句中却大写开头 | 真实的量和时长 | 网页版样例数据没有额度读数，这一行在网页版上根本不出现 | 句中的窗口名首字母改小写（`shortWindow` 挪进 `ui/parse.ts`） | `test/i18n.test.ts`「a quota window named mid-line…」 |
| 11-12 | 对话里比屏幕宽的表格左右滑不动，第二列一直被切掉；首页的空间筛选同样是横向滚动却能滑 | 平台差异 | 网页版和测试不跑安卓的触摸分发；表格外面包着长按复制的 Pressable，又在倒置（安卓转 180°）的对话列表里 | 本分支：表格和宽代码块里的内容自己接住触摸，加 `nestedScrollEnabled`。代价是长按表格不再复制整条消息 | 没有自动测试；真机同一张表滑动前后截图（`~/Documents/claude-pages/mooring/2026-11-12-table-scroll/v2-compare.png`） |
| 11-12 | 「终端最后几行」里 Claude 底栏的 `⏵⏵`（bypass permissions 前面那两个）显示成两个方框 | 平台差异 | 安卓等宽字体没有这个字形，网页版和电脑上有 | 代码已修（`36992dba`：换成手机字体有的 `▸` `●`），真机待复验 | 没有；真机截图 `2026-11-12-phone-walk-2/chat.terminal-1.png` |
| 11-17 | Codex 开在窄旁屏（`codex -a on-request -s read-only`）停在命令审批时，手机没认出审批：标题写「等你回答」、卡片只有「程序说在等你」和方向键，看不到命令和允许/拒绝。聊天页第一次发送只出软提示「确定要发，再点一次发送」，第二次发送把消息送进审批框，回车批准了 `touch`，命令执行了 | 真 CLI、宽度 | 识别要求审批问句在同一行、以「?」结尾（`screen_ask.rs` `is_question`、`agents/screen/codex.json` anchor）；旁屏窄，Codex 把问句折成「Would you like to run the」/「following command?」两行。夹具都是宽屏截的；本机 Codex 全局 `approval_policy = never`，平时也不弹审批 | `57e50d1f`（#387）：读屏前把折开的行拼回整行（`screen_lines`），问句可以跨行；底部开着带光标、1 号选项是 Yes 或 Trust 的菜单时，不管认没认出问句，消息都不打进去。RC3 真机复走过了（约 47 列，选项折行、问句没折）。窄窗格里 Codex 的信任框没修，只靠兜底挡住消息；折行的命令在手机上仍按屏幕行分开 | `screen_fixtures` 的 `codex/approval-narrow-wrapped-real.txt`（31 列）、`approval-narrow44-real.txt`（44 列）、`claude/approval-narrow-wrapped-real.txt`；`remote_message_safety::a_wrapped_or_unknown_approval_still_blocks_chat`；真机截图 `2026-11-17-phone-walk/card.sendguard-6.png`、`-8.png`（修前）、`2026-11-17-phone-walk-3/card.sendguard-5.png`～`-7.png`（修后） |
| 11-17 | Claude 命令审批卡片的说明多出半句「to auto mode" below」 | 真 CLI、版本 | Claude Code 审批框新加一行「Tip: auto mode handles these prompts for you — choose "switch to auto mode" below」，在终端里折成两行；解析只去掉了第一行，折下来的半句当成了说明 | #384 按审批框虚线量宽度，判 Tip 的续行；#387 补上「续行首字母不是大写」（31 列时 Tip 末行放不下说明首词，单靠宽度会把说明也删掉）。RC3 真机：100 列终端下说明干净；窄屏只有单元测试 | `screen_ask::tests::a_wrapped_tip_stays_out_of_the_note`、夹具 `claude/approval-narrow-wrapped-real.txt` |
| 11-17 | 审批开着时发消息被挡，提示「它在等你审批，先允许或拒绝，再发送消息」；按允许、命令跑完、状态回到等你验收后，这行红字还一直挂着，直到下一张审批卡片出现才消失 | 时序和进程寿命 | 测试只看挡没挡住，不看审批结束后提示有没有收回 | #384：挡住发送的那行字记下来，挡它的审批或选择框没了就清掉。RC3 真机复验过：按允许、命令跑完后红字收起 | `mobile/test/waits.test.ts`「the line that stopped a send」 |
| 11-17 | 开着面容锁时把 App 切到后台再回来，弹出验证后点取消：屏幕上只剩解锁页，但无障碍树（uiautomator，也就是 TalkBack 读的那份）里仍能读到下面主页的会话标题和目录。冷启动时没有这个问题 | 时序和进程寿命 | 冷启动在解锁前不挂载页面（#330）；热回来时页面已经挂着，锁页只是盖在上面，没对读屏隐藏。测试和截图只看画面 | 没修，只记下 | 没有；真机截图和 UI dump `2026-11-17-phone-walk-2/me.lock-8.png`、`.xml` |
| 11-18 | 新任务页选了不在任何空间里的目录后，「加图片」「从剪贴板粘贴」「接着上次的对话」被底部固定的「开始」栏盖住，点了没反应，要先往上滑 | 平台差异、真实数据 | 选了这种目录会多出「不在任何空间里」提示和「设为新空间」一行，表单变长；网页版和样例数据的目录都在空间里，表单够短 | 没修，只记下 | 没有；真机截图 `2026-11-18-phone-walk/new.task-19.png` |
| 11-18 | 新任务页滚动时点了表单里的位置，误点到「把 … 设为新空间」，电脑上真的多了一个空间，没有确认 | 真实数据 | 这一行只在目录不在任何空间里时出现，样例数据里没有；按一下就直接新建，不问 | 没修，只记下 | 没有；`remote-log.jsonl` 13:27 的 addspace |
| 11-18 | 批准计划后 Claude 退回逐个确认（电脑上是 manual mode），手机对话设置里权限还写「只做计划」；打开权限列表时「正在用」又标在逐个确认上，两处不一致 | 真 CLI | 权限那一行显示手机上次设的值，没跟着 CLI 自己切的模式走；测试只测手机设一次之后的显示 | 没修，只记下 | 没有；真机截图 `2026-11-18-phone-walk/chat.sheet-4.png` |
| 11-18 | 手机上开 Codex 新任务，如果同目录另一个 Codex 窗格的画面上正好有同一句任务原文（走查时从那个窗格复制来的），新终端一直不绑：agent、标题都是空，手机写「codex · 空闲 · 未关联对话」，发不了消息 | 真 CLI、真实数据 | 按画面认对话时，一个对话对得上两个同目录窗格，就按设计哪个都不绑；测试里不会两个窗格同时显示同一句 | 没修，只记下（mooring-25 查实原因，RC4 记非阻断） | 没有；真机截图 `2026-11-18-phone-walk/new.task-11.png`、`-12.png` |
| 11-18 | Claude 里点「立刻发送」后紧接着中断，消息留在待确认气泡；用深链切到另一个 Codex 测试会话，新标题、历史、审批已经切换，旧 Claude 的气泡仍显示「已立刻发送」，直到 Codex 回复后还在 | 时序和进程寿命 | 普通走查先退回首页再进另一段对话，未覆盖同一个聊天页面直接换深链参数；`chat.tsx` 的历史会重读，但 `pending` 和输入组件状态仍属于同一个 React 实例 | 待发送气泡改为按电脑和终端存（`mobile/src/store/pending.ts`），换会话不带过去，切回来还在；输入框草稿和已选图片仍属于聊天页面，没改。实测的是 v0.1.3（`234772d2`）；未观察到文字实际发给另一会话，不能把显示串话说成命令串发 | `mobile/test/pending.test.ts`，真机未复验；本地私有证据 `~/Documents/claude-pages/mooring/2026-11-18-phone-walk-2/card.sendguard-1.png`、`card.sendguard-2.png`、`card.reject-2.png`。需复验待发送气泡、草稿和图片在换会话时的归属 |
| 11-19 | 从手机发一张图加一句话给 Claude：缩略图在，但 Claude 只收到文字、答「没有收到图片」，`[Image #1]` 留在电脑上 Claude 的输入框里，下一条消息才带上；同一流程再发一次就正常读出 WALK 4217（两次里一次失败） | 时序和进程寿命 | 后台先粘图片路径、紧接着发文字加回车，中间不等（`remote.rs` 的 send、`remote/images.rs` 的 paste）；Claude 读文件、换成附件是异步的，机器负载 90～150 时回车先到。11-18 负载低时这一项过了，测试里的假 Agent 不做这一步 | #451：粘完图片后看画面，等新的 `[Image #N]` 出现（Claude 编号往上数，Codex 每条从 #1 数，按编号或标记个数认）再发文字和回车，最多等 5 秒。真 Claude、真 Codex 各发 2 次都带上图，约多等 0.3 秒 | `tests/remote_images.rs` the_words_wait_until_the_cli_shows_the_picture（假 Agent 1 秒后才画出图片标记）；真机截图 `2026-11-19-phone-walk-2/chat.image-*`；修后没在真手机上重走 |
| 11-20 | build 1479 手机里的逐个确认权限说明在 `internet` 后截断，用户看不到后续权限条件 | 真机排版、真实说明 | 权限选项说明和模型说明共用两行上限；短文案样例覆盖不到 | 说明按可用宽度完整换行。build 1537 手机真机看到「逐个确认」读到 outside the workspace | 本机走查 `2026-11-20-phone-walk` 的 `chat.sheet`，修后 `2026-11-20-phone-walk-4/chat.sheet-1.png`；截图不进仓库 |

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
