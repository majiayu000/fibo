# Mooring 手机端竞品调研

研究快照：2026-11-03。这份文档只写竞品，我方要改什么见 [02-my-plan.md](02-my-plan.md)。
对照基线：Mooring 仓库 main@d88045ec 的手机端设计稿（`specs/phone-app/design.html`，现 `docs/phone-app/design.html`）和已实现的 App（`mobile`）。

图例（结论后方括号里的证据等级）：`[E2]` 只看了官方 README、文档或商店页；`[E3]` 读过固定 commit 的源码；`[D1]` 只有零星公开讨论。没有装上竞品实际使用，所以没有任何「谁更好用、谁更快」的结论。

## 1. 证据边界

| 材料 | 能支持的判断 | 不能由此推出的结论 |
|---|---|---|
| 5 个开源仓库的固定 commit 源码（Happy、Paseo、coucou、Nexting、codex-relay） | 功能是否有实现、实现方式 | 功能在用户手机上稳定可用 |
| 官方文档：Claude Code Remote Control、OpenAI「Work with Codex from anywhere」 | 官方宣称的能力和限制 | 实际体验、推送是否及时 |
| App Store 和 README 里的官方截图（T3 Code、Happy、Paseo、coucou、codex-relay） | 外观、版式、信息怎么排 | 截图是宣传图，可能和真实使用时不同 |
| GitHub 星数和最近更新时间（2026-11-03 抓取） | 关注度和是否在维护 | 用户数、满意度 |
| HN 三帖评论抽样 | 有用户提过某个痛点 | 痛点普遍、最新版仍存在 |

没做：安装竞品、登录账号、对照实测、安全测试。闭源产品（Omnara 托管版、Forge Remote、MobileCLI）只看了商店页或搜索摘要。

## 2. 候选池（20 个）

按形态分三组。星数为 2026-11-03 抓取。

**专做「手机遥控本机 Agent」的 App**

| 产品 | 形态 | 关注度 | 与 Mooring 的关系 |
|---|---|---|---|
| [T3 Code](https://github.com/pingdotgg/t3code) | iOS/Android App + Web + 桌面 + 本机服务，MIT | 42,823★ | 关注度最高；手机端同样是 Expo SDK 58，外观最讲究。第一轮漏掉了，第二轮补上 |
| [Happy](https://github.com/slopus/happy) | iOS/Android/Web App + CLI 包装 + 自托管中继，MIT | 24,464★，App Store 4.9（1,000+ 评分，README 自述） | 最直接的同类：同样是 Expo App、端到端加密中继 |
| [coucou](https://github.com/Louis-CFM/coucou) | Mac 刘海助手 + iPhone/Android App，MIT | 6,916★ | 定位最接近：盯住 Agent、随时审批；系统集成最深 |
| [codex-relay](https://github.com/gronxb/codex-relay) | 本机中继 + 手机 App，Apache-2.0 | 726★ | 只做 Codex，架构同类 |
| [Nexting](https://github.com/Nexting-ai/nexting) | iPhone App + 可穿戴硬件，MIT | 2,149★ | 主打语音派任务，不是聊天 |
| Claude 官方 Remote Control | Claude App / claude.ai/code 连本机 Claude Code | 官方 | 官方方案，记录存 Anthropic 服务器 |
| ChatGPT 里的 Codex（预览） | ChatGPT App 连本机 Codex，2026-05-14 发布 | 官方 | 官方方案，暂不支持 Windows |
| [Omnara](https://github.com/omnara-ai/omnara) | 托管平台 + Web/手机，Apache-2.0 | 3,346★ | 托管路线；HN 上自述没有端到端加密 |
| Forge Remote、MobileCLI | 闭源 iPhone App | 商店页 | 只看了商店页 |
| [claude-remote-approver](https://github.com/yuuichieguchi/claude-remote-approver) | 用 ntfy 推送审批 | 73★ | 只做审批这一件事 |

**完整的多端 Agent 工作台（手机是其中一端）**

| 产品 | 形态 | 关注度 |
|---|---|---|
| [Paseo](https://github.com/getpaseo/paseo) | 本机后台 + 中继 + 桌面/手机/Web/CLI | 22,879★ |
| [CloudCLI / claudecodeui](https://github.com/siteboon/claudecodeui) | Web/手机界面，AGPL-3.0 | 14,412★ |
| [Happier](https://github.com/happier-dev/happier) | Happy 分支，多端 | 2,123★ |
| [Nimbalyst](https://github.com/nimbalyst/nimbalyst) | 桌面工作区 + 手机看板 | 2,016★ |
| [agent-of-empires](https://github.com/agent-of-empires/agent-of-empires) | TUI + Web | 3,404★ |
| [collie](https://github.com/AltanS/collie)、[ovis](https://github.com/devswha/ovis) | 同类终端工具 的手机 / 浏览器客户端 | 1,556★ / 1,834★ |

**替代物（用户现在不用 App 时怎么做）**

| 做法 | 证据 |
|---|---|
| Tailscale + SSH 客户端（Termius、Blink）+ tmux | HN 上有人说手机上用终端界面「far from comfortable」（[46742298](https://news.ycombinator.com/item?id=46742298)） |
| 让 Agent 监听 Slack / Telegram 私信 | HN [46991658](https://news.ycombinator.com/item?id=46991658)；Claude Code 官方也有 Channels |
| [VibeTunnel](https://github.com/amantus-ai/vibetunnel)：浏览器里开终端 | 4,706★，最近更新 2026-08-05 |

## 3. 功能对照

「有」都附了出处；「查过没有」写明查了哪里；没查的写「未查」。Mooring 一栏是设计稿状态，不是已实现。

| 功能 | Happy | Paseo | coucou | codex-relay | Claude 官方 | Mooring 设计稿 |
|---|---|---|---|---|---|---|
| 通知上直接审批 | 查过没有：App 忽略非默认操作 [E3] | 未查 | 有，按钮都要 Face ID [E3] | 未查 | 文档没写 [E2] | 有（设置页写了） |
| 人在电脑前就不推送 | 未查 | 有：3 分钟内在用、且正看着这个 Agent 就不推 [E3] | 有：Mac 锁屏才转到 iPhone [E2] | 未查 | 未查 | 缺 |
| 审批选项 | 允许 / 这类修改都允许 / 本会话这个工具都允许 / 全部放行 / 拒绝 [E3] | 未查 | 允许 / 拒绝 / 总是 [E2] | 有 [E2] | 有 [E2] | 允许一次 / 会话里都允许 / 拒绝并说明 |
| 每次允许都要生物识别 | 未查 | 未查 | 有，不能关 [E3] | 未查 | 未查 | 只有「打开 App 时」可选 |
| 审批决定记录 | 未查 | 未查 | 有，本地保存 [E3] | 未查 | 未查 | 缺 |
| 切换权限模式（计划 / 接受编辑 / 放行） | 有 [E3] | 未查 | 未查 | 未查 | 启动参数 [E2] | 缺 |
| 切换模型 | 有，含思考强度 [E3] | 有 [E2] | — | 未查 | 有 [E2] | 缺 |
| 改动 diff | 有，独立页面 [E3] | 有 [E2] | 有，点开看全部 [E2] | 有 [E2] | 有 diff 面板 [E2] | 只有一行摘要 |
| 发图片 / 文件 | 有，相册 [E3] | 有 [E2] | 拖文件（Mac 端）[E2] | 未查 | 有 [E2] | 缺 |
| 语音 | 有，语音助手能发消息、能审批 [E3] | 有 [E2] | 有，听写和 Siri [E2] | 未查 | 未查 | 缺 |
| 锁屏实时活动 / 小组件 / Siri | 未查 | 未查 | 有 [E3] | 未查 | 未查 | 缺 |
| 新任务选目录 | 最近目录 + 手输路径 [E3] | 模糊搜目录 / 从 GitHub 克隆 / 新建目录 [E3] | — | 在启动目录里 | 启动时所在目录 [E2] | 浏览 + 搜索 + 新建 |
| 新任务开独立工作树 | 有 [E3] | 有 [E2] | — | — | 有，`--spawn worktree` [E2] | 缺 |
| 忙时排队发送 | 未查 | 未查 | 未查 | 有 [E2] | 有 [E2] | 缺（没有显示排队状态） |
| 手机上预览本机网页 | 未查 | 有内置浏览器 [E2] | — | 有 [E2] | 未查 | 缺 |
| 配对时电脑再确认 | 未查 | 未查 | 未查 | 有，电脑上输审批码 [E2] | 账号登录 | 缺 |
| 端到端加密 | 有 [E2] | 有 [E2] | 推送只带哈希 [E3] | 局域网直连 [E2] | 没有：记录存 Anthropic [E2] | 有（Noise） |
| 接上已经在跑的会话 | 要用 `happy claude` 启动 [E2] | 由后台启动 [E2] | 读钩子，不限启动方式（只看和审批）[E2] | 由中继启动 [E2] | 有：会话里输入 `/remote-control` [E2] | 有，限于在 Mooring 终端里跑的 Agent |

## 4. 重点竞品做得好的地方

**T3 Code**（[固定 commit](https://github.com/pingdotgg/t3code/tree/cefb544af3c98224874419198c0359b652ca3c94)）
- 手机端和 Mooring 同样用 Expo SDK 58，样式用 uniwind（React Native 里的 Tailwind），iOS 上用系统的玻璃效果（[`package.json`](https://github.com/pingdotgg/t3code/blob/cefb544af3c98224874419198c0359b652ca3c94/apps/mobile/package.json)）[E3]。
- 字体是 DM Sans，字号从 11 到 30 共 9 档，每档都定了行高（[`global.css`](https://github.com/pingdotgg/t3code/blob/cefb544af3c98224874419198c0359b652ca3c94/apps/mobile/global.css)）；5 套配色（t3-chat、grove、ocean、ember、iris），每套都有浅色和深色 [E3]。
- 首页把会话分成几组：正常的、「未发送」（断网时写的话，标「重连后发送」）、「稍后提醒」、「已处理」，后两组默认折叠；会话行可以左右滑动（[`features/home`](https://github.com/pingdotgg/t3code/blob/cefb544af3c98224874419198c0359b652ca3c94/apps/mobile/src/features/home)）[E3]。
- 「稍后提醒」可以按时长或指定时间（[`CustomSnoozeSheet.shared.tsx#L53`](https://github.com/pingdotgg/t3code/blob/cefb544af3c98224874419198c0359b652ca3c94/apps/mobile/src/features/threads/CustomSnoozeSheet.shared.tsx#L53)）；审批是「允许一次 / 本会话允许 / 拒绝」（[`PendingApprovalCard.tsx#L28-L30`](https://github.com/pingdotgg/t3code/blob/cefb544af3c98224874419198c0359b652ca3c94/apps/mobile/src/features/threads/PendingApprovalCard.tsx#L28-L30)）[E3]。
- 会话里还有：Agent 计划单独成卡片、排队的消息能再编辑、在 diff 的某一行写评论发给 Agent、子 Agent 状态条（[`features/threads`](https://github.com/pingdotgg/t3code/blob/cefb544af3c98224874419198c0359b652ca3c94/apps/mobile/src/features/threads)、[`features/review`](https://github.com/pingdotgg/t3code/blob/cefb544af3c98224874419198c0359b652ca3c94/apps/mobile/src/features/review)）[E3]。
- 锁屏实时活动列出所有 Agent 和状态，标出几个要你处理（[`agentLiveActivity.ios.ts`](https://github.com/pingdotgg/t3code/blob/cefb544af3c98224874419198c0359b652ca3c94/apps/mobile/src/features/agent-awareness/agentLiveActivity.ios.ts)）[E3]。
- 终端页有一排 esc / tab / ctrl / 方向键，电脑上开了端口会弹「5173 端口已就绪 · 打开」[E2，来自商店截图]。


**Happy**（[固定 commit](https://github.com/slopus/happy/tree/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1)）
- 新会话一屏有 8 个选择器：电脑、路径、工作树、Agent、模型、思考强度、权限模式、设置（[`new/index.tsx#L140`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/app/%28app%29/new/index.tsx#L140)），选工作树时当场创建（[`#L1590`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/app/%28app%29/new/index.tsx#L1590)）[E3]。
- 路径只提供「最近用过的地方」加手动输入，不能逐层浏览（[`#L972`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/app/%28app%29/new/index.tsx#L972)）[E3]。
- 审批有 4 种允许方式（[`PermissionFooter.tsx#L142-L211`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/components/tools/PermissionFooter.tsx#L142-L211)），权限模式 5 档（[`modelModeOptions.ts#L118`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/components/modelModeOptions.ts#L118)）[E3]。
- 语音助手接 ElevenLabs，能代你发消息和处理审批（[`_default.ts#L770`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/text/_default.ts#L770)）[E3]。
- 推送只能点开跳到会话，App 忽略通知上的操作按钮（[`_layout.tsx#L352`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/app/_layout.tsx#L352)）[E3]。
- HN 上贡献者承认曾有「reliability issues and feature stagnation」（[46757485](https://news.ycombinator.com/item?id=46757485)）[D1]。

**Paseo**（[固定 commit](https://github.com/getpaseo/paseo/tree/b507fc198616bfcaf1753f461f56ba97f2714852)）
- 推送看「人在哪」：任一设备 3 分钟内有操作、App 在前台且正看着这个 Agent，就不推送（[`agent-attention-policy.ts#L3`](https://github.com/getpaseo/paseo/blob/b507fc198616bfcaf1753f461f56ba97f2714852/packages/server/src/server/agent-attention-policy.ts#L3)）[E3]。
- 添加项目有三条路：搜目录、从 GitHub 克隆、新建目录（[`add-project-flow/model.ts#L29`](https://github.com/getpaseo/paseo/blob/b507fc198616bfcaf1753f461f56ba97f2714852/packages/app/src/add-project-flow/model.ts#L29)）[E3]。
- 目录搜索在后台做：默认 30 条、最多 100 条，最深 12 层，最多扫 2 万个条目，缓存 8 秒，跳过 gitignore 和常见依赖目录（[`directory-suggestions.ts#L105-L108`](https://github.com/getpaseo/paseo/blob/b507fc198616bfcaf1753f461f56ba97f2714852/packages/server/src/utils/directory-suggestions.ts#L105-L108)）[E3]。
- 每个请求类型对应一项权限，比如读工作区、写工作区、管理后台（[`operation-permissions.ts`](https://github.com/getpaseo/paseo/blob/b507fc198616bfcaf1753f461f56ba97f2714852/packages/server/src/server/authorization/operation-permissions.ts)）[E3]。
- 还有定时任务、插件、内置浏览器 [E2]。

**coucou**（[固定 commit](https://github.com/Louis-CFM/coucou/tree/e9fc337f0fa1b31698474ef87d4ad0d458dbb5fa)）
- 通知上的允许、查看、拒绝、回复都要求 Face ID；Agent 提的单选问题，最多 4 个选项直接变成通知按钮（[`AgentNotifier.swift#L61-L90`](https://github.com/Louis-CFM/coucou/blob/e9fc337f0fa1b31698474ef87d4ad0d458dbb5fa/NotchBuddy/Sources/Phone/AgentNotifier.swift#L61-L90)）[E3]。
- 允许前必须过 Face ID 或密码，代码注释写「Never skipped」（[`Decisions.swift#L30`](https://github.com/Louis-CFM/coucou/blob/e9fc337f0fa1b31698474ef87d4ad0d458dbb5fa/NotchBuddy/Sources/Phone/Decisions.swift#L30)）；每次决定记一条（[`#L9`](https://github.com/Louis-CFM/coucou/blob/e9fc337f0fa1b31698474ef87d4ad0d458dbb5fa/NotchBuddy/Sources/Phone/Decisions.swift#L9)）[E3]。
- 推送中转只看得到状态和命令的 SHA-256 指纹，看不到项目名、命令、路径（[`relay/README.md`](https://github.com/Louis-CFM/coucou/blob/e9fc337f0fa1b31698474ef87d4ad0d458dbb5fa/relay/README.md)）[E3]。
- 锁屏实时活动上能直接允许或拒绝（[`MochiLiveActivity.swift`](https://github.com/Louis-CFM/coucou/blob/e9fc337f0fa1b31698474ef87d4ad0d458dbb5fa/NotchBuddy/Sources/Widgets/MochiLiveActivity.swift)、[`DenyApprovalIntent.swift`](https://github.com/Louis-CFM/coucou/blob/e9fc337f0fa1b31698474ef87d4ad0d458dbb5fa/NotchBuddy/Sources/Phone/DenyApprovalIntent.swift)）[E3]；小组件、Siri、专注模式过滤 [E2]。

**codex-relay**（[固定 commit](https://github.com/gronxb/codex-relay/tree/b2093fc1ed5df383344b47ca948c9c4c2e2760b7)）：可以分别开关「一轮做完」和「需要你处理」两种推送；Agent 忙时发的话会排队，并显示排队状态；手机上能预览 git 改动、本机网页和文件；配对时手机显示一个码，要在电脑上 `approve` 才算配上 [E2]。

**Claude 官方 Remote Control**（[文档](https://code.claude.com/docs/en/remote-control)，2026-11-03 读取）：服务端模式下，新会话可以各开一个 git 工作树，最多 32 个并发；手机上能发图片和文件、`@` 补全本机路径、看 diff、切模型；推送分「Claude 自己判断」和「需要你操作」两档，除此之外不能按事件配置；会话记录存在 Anthropic 服务器上 [E2]。

**ChatGPT 里的 Codex**（[OpenAI 公告](https://openai.com/index/work-with-codex-from-anywhere/)，2026-05-14）：扫码配对本机，手机上可以审批命令、切模型、看截图、终端输出、diff 和测试结果，也能开新任务；预览版，暂不支持 Windows [E2]。

## 5. 用户声音（HN 抽样，3 帖）

- 加密是硬门槛：「If its encrypted end-to-end than I'm in」（[46992325](https://news.ycombinator.com/item?id=46992325)）。Omnara 回复说没有真正的端到端加密，因为同步、推送和语音都要读内容（[46992557](https://news.ycombinator.com/item?id=46992557)）[D1]。
- 回到电脑前接不上同一个环境：「can't sit back down at my computer and cd/ssh/tmux into that same environment」（[46994034](https://news.ycombinator.com/item?id=46994034)）[D1]。
- 手机上的终端界面难用，安卓上找不到好工具（[46742298](https://news.ycombinator.com/item?id=46742298)）[D1]。

三帖总共几十条评论，只能说明有人提过，不能说明普遍。

## 6. 外观对比

截图都是各家自己放在 App Store 或 README 里的宣传图，存在 [./evidence/look/](./evidence/look/)。

| | T3 Code | codex-relay | Paseo | coucou | Mooring 设计稿 |
|---|---|---|---|---|---|
| 主色调 | 浅色为主，也有深色；近黑 `#0a0a0a`、卡片 `#111111`、边框 `#191919` | 深色 | 深色 | 深色 | 只有深色 |
| 会话列表 | 平铺的行，细线分隔；项目名 → 标题 → 等宽的分支和机器名；状态是右侧一个彩色词 | — | 按仓库 → 分支分组的树，带 PR 编号和状态 | — | 一张张卡片 |
| 自己的消息 | 浅灰气泡 | 深灰气泡 | — | — | 饱和蓝色气泡 |
| Agent 的回复 | 不放气泡，整宽正文；过程折成「Worked for 2m ›」 | 不放气泡，上方标「Codex」；工具调用是小标签 | — | — | 白色气泡 |
| 输入框 | 悬浮胶囊：+、输入、麦克风、发送 | 大圆角卡片：+、权限模式标签、模型标签、上下文用量圆环；在跑时发送键变成停止键 | — | — | 单行输入 + 发送，上面一排快捷按钮 |
| 标题栏 | 圆形返回键 + 一组图标胶囊（终端、文件、git） | 圆形按钮 | 汉堡菜单 | 圆形返回键 | 普通返回箭头 |
| 审批 | 卡片里三个按钮 | — | — | 大字号命令，「拒绝」暗红、「允许」绿色带 Face ID 图标，下方写「2 分钟后失效」 | 白色主按钮、灰色次按钮 |
| 看改动 | 每个文件一张卡片，带「看过」勾选框；底部固定卡片「2 个文件 +15 −2 · 612 个测试通过」，「要求修改 / 批准并合并」 | 有 | 「改动 / 文件」两个标签页，右上「查看 PR」 | 有 | 没有 |
| 电脑列表 | 底部弹出的面板，每台一个开关 | — | 设置里的卡片 | — | 独立的一页 |

几家共同的做法：
- Agent 的长回复不放进气泡；
- 自己的消息用中性灰；
- 分支、路径、命令一律用等宽字体；
- 输入框是一个大圆角容器，把模式、模型这些控件收进去；
- 列表不用卡片，用平铺的行。

Mooring 设计稿只在「等宽字体」一点上和主流一致，其余四点都相反。原因是沿用了第一版「聊天气泡」的写法，并不是有意的取舍。

## 7. 关键设计对照：通知

| 产品 | 什么时候推 | 通知上能做什么 | 安全 |
|---|---|---|---|
| Paseo | 人不在任何设备前才推 | 未查 | 未查 |
| coucou | Mac 锁屏后转到 iPhone | 允许 / 查看 / 拒绝 / 选答案 / 回复 | 每个按钮都要 Face ID；中转只见哈希 |
| Claude 官方 | Claude 自己判断 + 需要你操作 | 文档没写 | 记录在 Anthropic |
| Happy | 需要权限或出错时 | 只能点开 | 端到端加密 |

主流做法：推送分「需要你」和「做完了」两类；最好的做法是再看人在不在电脑前，并且能在通知上直接处理。

## 8. 关键事实核验

| 事实 | 影响哪条结论 | 来源与日期 | 结果 |
|---|---|---|---|
| Happy 通知上不能直接审批 | 「通知上直接审批」是差异点之一 | [`_layout.tsx#L352`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/app/_layout.tsx#L352) 忽略非默认操作；推送服务虽支持 categoryId，App 不处理；2026-11-03 | 成立 |
| Happy 新会话不能逐层浏览目录 | Mooring 的目录浏览算差异点 | 反查：搜了 `listDirectory`，只在已开会话的文件页用（[`sync/ops.ts#L1067`](https://github.com/slopus/happy/blob/b65dcf974da2ed2d5b02d2d21e006cc32c3ad5d1/packages/happy-app/sources/sync/ops.ts#L1067)）；2026-11-03 | 成立，限于新会话流程 |
| Paseo 也没有逐层浏览 | 同上 | 只读了 add-project-flow 的页面类型（搜目录、克隆、新建目录），没读每页细节 | 无法核实：搜目录页可能带浏览，结论降为「Happy 没有，Paseo 未确认」 |
| Claude 官方没有端到端加密 | Mooring 的加密是差异点 | 官方文档 Security 节：记录存 Anthropic 服务器；2026-11-03 | 成立 |
| 「接上已在跑的会话」是 Mooring 独有 | 差异点 | 反查：Claude 官方文档写明已开的会话里输入 `/remote-control` 就能接上；coucou 读钩子不限启动方式；2026-11-03 | 不成立：官方和 coucou 都能接上已在跑的会话；Mooring 也只限于在它的终端里跑的 Agent |
| T3 Code 有正式上架的手机 App | 候选池遗漏、外观主要参照对象 | README 链接 App Store 和 Google Play；商店页能打开并有 6 张截图；源码 `apps/mobile`；2026-11-03 | 成立 |
| coucou 允许必须 Face ID | 建议改成每次审批都验证 | [`Decisions.swift#L30`](https://github.com/Louis-CFM/coucou/blob/e9fc337f0fa1b31698474ef87d4ad0d458dbb5fa/NotchBuddy/Sources/Phone/Decisions.swift#L30) | 成立 |

## 9. 可借鉴与不能照搬

- 可以借鉴：Paseo 的「人在哪」推送策略和目录搜索限额；coucou 的带按钮通知、每次允许都验证、推送只带哈希；Happy 的审批分级和权限模式；官方的工作树选项。
- 不照搬：Happy 和 Omnara 的语音助手要把内容交给第三方语音服务，和 Mooring 的端到端加密冲突；coucou 的邮件、加密货币、音乐等与 Agent 无关的功能；多人协作。
- 许可：Happy、coucou、Nexting 是 MIT；Paseo 的许可证 GitHub 识别为 NOASSERTION，没细看；CloudCLI 是 AGPL-3.0，不能直接拿代码。这次只借鉴设计，不复用代码。
