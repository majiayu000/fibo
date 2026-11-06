# 手机端首页和功能：竞品调研

研究快照：2026-11-07。这份只写竞品，Mooring 要改什么见 [02-my-plan.md](02-my-plan.md)。上一轮的候选池和完整功能表见 [20261103-s1](../20261103-s1/01-competitor-research.md)。

基线：Mooring 手机 App，首页是固定版本的 [`index.tsx`](https://github.com/majiayu000/mooring/blob/7e8aebba2d5c35bd1ff03f4be44e88149ded281e/mobile/src/app/index.tsx)。

图例：`[E2]` 只看了官方文档、README 或商店截图；`[E3]` 读过固定 commit 的源码。没有装上任何竞品实际使用，所以没有「谁更好用」的结论。

## 1. 各家怎么组织首页

| 产品 | 导航 | 打开先看到 | 首页上的其他东西 |
|---|---|---|---|
| [T3 Code](https://github.com/pingdotgg/t3code)（49,224★） | 三个区：Threads、Pulls、Usage（[源码](https://github.com/pingdotgg/t3code/blob/6577c6b6185167c0e502fa01f9e8e8f2dee6ff25/apps/mobile/src/features/layout/workspace/navigation-tab-order.ts#L1)）[E3] | 会话列表，分组：置顶、在跑、未发送、稍后提醒、已处理（[分组](https://github.com/pingdotgg/t3code/blob/6577c6b6185167c0e502fa01f9e8e8f2dee6ff25/apps/mobile/src/features/threads/thread-list-v2-items.tsx)）[E3] | 搜索、电脑和项目筛选（[`HomeScreen.tsx#L732`](https://github.com/pingdotgg/t3code/blob/6577c6b6185167c0e502fa01f9e8e8f2dee6ff25/apps/mobile/src/features/home/HomeScreen.tsx#L732)）；划掉后可撤销（[`ThreadUndoNotice.tsx`](https://github.com/pingdotgg/t3code/blob/6577c6b6185167c0e502fa01f9e8e8f2dee6ff25/apps/mobile/src/features/home/ThreadUndoNotice.tsx)）[E3] |
| [coucou](https://github.com/Louis-CFM/coucou)（6,999★） | 四个标签：Agents、Services、History、Search（[`HomeView.swift#L19-L30`](https://github.com/Louis-CFM/coucou/blob/b1bc09de7f7c24a80bd8eca5729b8bee60f66402/NotchBuddy/Sources/Phone/HomeView.swift#L19-L30)）[E3] | 一张「今天」卡片，下面是按紧急程度排的会话（[`#L112`](https://github.com/Louis-CFM/coucou/blob/b1bc09de7f7c24a80bd8eca5729b8bee60f66402/NotchBuddy/Sources/Phone/HomeView.swift#L112)、[`#L124`](https://github.com/Louis-CFM/coucou/blob/b1bc09de7f7c24a80bd8eca5729b8bee60f66402/NotchBuddy/Sources/Phone/HomeView.swift#L124)）[E3] | 今天卡片写轮数、文件数、增删行数、批准次数；在行上滑动就能允许或拒绝（[`#L128`](https://github.com/Louis-CFM/coucou/blob/b1bc09de7f7c24a80bd8eca5729b8bee60f66402/NotchBuddy/Sources/Phone/HomeView.swift#L128)）[E3] |
| [CC Pocket](https://github.com/K9i-0/ccpocket)（1,093★） | 手机上是标题栏图标加右下角「New」按钮（[截图](evidence/look/ccpocket-screens.png)）[E2] | 「Running」列表，「Needs You」的卡片里直接批准（[截图](evidence/look/ccpocket-screens.png)）[E2] | 额度一行写在 Running 标题旁（[`usage_summary.dart#L52`](https://github.com/K9i-0/ccpocket/blob/9c5cfdd29852689f72f50bf4dc1ec15526206f07/apps/mobile/lib/features/session_list/widgets/usage_summary.dart#L52)）；最近会话能按项目、工具、时间、置顶筛（[`session_filter_bar.dart#L61`](https://github.com/K9i-0/ccpocket/blob/9c5cfdd29852689f72f50bf4dc1ec15526206f07/apps/mobile/lib/features/session_list/widgets/session_filter_bar.dart#L61)）和搜索（[`session_list_screen.dart#L171`](https://github.com/K9i-0/ccpocket/blob/9c5cfdd29852689f72f50bf4dc1ec15526206f07/apps/mobile/lib/features/session_list/session_list_screen.dart#L171)）[E3] |
| [Happy](https://github.com/slopus/happy)（24,520★） | 未查完整结构 | 安卓版是按日期分组的会话列表（[`HomeData.kt#L58`](https://github.com/slopus/happy/blob/665b1f14f3b35d89dead49b7c5c97b58df42f48d/packages/happy-android/app/src/main/kotlin/com/slopus/happy/ui/home/HomeData.kt#L58)）[E3] | 底部常驻输入框，直接新建会话、机器人或任务（[`HomeDockKind.swift#L3-L10`](https://github.com/slopus/happy/blob/665b1f14f3b35d89dead49b7c5c97b58df42f48d/packages/happy-ios/Sources/HappyCore/Home/HomeDockKind.swift#L3-L10)）[E3] |
| [Paseo](https://github.com/getpaseo/paseo)（23,287★） | 侧边抽屉（[截图](../20261103-s1/evidence/look/paseo-mobile.png)）[E2] | 按仓库和分支分组的树，带 PR 编号和状态（[截图](../20261103-s1/evidence/look/paseo-mobile.png)）[E2] | 会话历史页有搜索，多台电脑时才显示电脑筛选（[`sessions-screen.tsx#L121`](https://github.com/getpaseo/paseo/blob/73448d483ed8de1103785d6a1180eee585a716fb/packages/app/src/screens/sessions-screen.tsx#L121)）[E3] |
| [Nimbalyst](https://github.com/nimbalyst/nimbalyst)（2,030★） | 先选项目，再进会话（[`SessionListView.swift#L144`](https://github.com/nimbalyst/nimbalyst/blob/826adb4f93dbe86db35b76ce2502a3a33abe0489/packages/ios/NimbalystNative/Sources/Views/SessionListView.swift#L144)）[E3] | 某个项目的会话 | 按阶段筛选、搜索、归档（[`#L292`](https://github.com/nimbalyst/nimbalyst/blob/826adb4f93dbe86db35b76ce2502a3a33abe0489/packages/ios/NimbalystNative/Sources/Views/SessionListView.swift#L292)）[E3] |
| [codex-relay](https://github.com/gronxb/codex-relay)（729★） | 抽屉里是会话列表（[`_layout.tsx#L6`](https://github.com/gronxb/codex-relay/blob/59789de0aa0da9ab6db5d3f31287098613c94446/apps/mobile/src/app/%28drawer%29/_layout.tsx#L6)）[E3] | 没查到单独的首页 | — |
| [Claude App](https://code.claude.com/docs/en/mobile) | 大 App 里一个 Code 标签 [E2] | 会话列表；另有 Projects，用来看「哪些完成了、哪些要你」[E2] | 不能选 Bypass 权限 [E2] |
| [ChatGPT 里的 Codex](https://learn.chatgpt.com/docs/mobile.md) | 大 App 里一个 Codex 入口 [E2] | 云端和电脑上的任务，并标出要你输入的 [E2] | 布局文档没写 |
| [Cursor iOS](https://cursor.com/docs/cloud-agent/mobile) | 文档叫 inbox，没写布局 [E2] | Agent 收件箱 [E2] | 锁屏实时活动最多 8 个；一轮做完就推送；能在手机上审 PR 和合并 [E2] |

文档和截图都是各家自己发布的，文档没写的布局标为未知，没有猜。

## 2. 共同做法和差异

- **首页基本都是排好序的会话列表。** 读了 7 家的首页或导航源码和截图：T3、coucou、CC Pocket、Happy、Paseo 打开就是会话列表或按仓库分组的树；Nimbalyst 先选项目再进列表；codex-relay 的列表在抽屉里。这 7 家里，查到的范围内没有一家用数字格子做概览。最接近的是 coucou 的今天卡片，也只是列表上方的一行数字。Claude、Codex、Cursor 的文档没写布局，无法确认。
- **急的放最前面，其余折叠或靠后。** T3 用分组，coucou 按紧急程度排序，CC Pocket 用 Needs You 状态标记。
- **列表上直接批准。** coucou（滑动）和 CC Pocket（按钮）都能做，Mooring 也能。T3 的宣传图上列表只写一个状态词，是否能在列表批准未查。
- **搜索是常见配置。** T3、coucou、CC Pocket、Paseo、Nimbalyst 都有；Mooring 的首页和看板没有（查过 [`index.tsx`](https://github.com/majiayu000/mooring/blob/7e8aebba2d5c35bd1ff03f4be44e88149ded281e/mobile/src/app/index.tsx) 和 [`board.tsx`](https://github.com/majiayu000/mooring/blob/7e8aebba2d5c35bd1ff03f4be44e88149ded281e/mobile/src/app/board.tsx)，只有目录页和工具页有搜索）。
- **额度放法分两种。** CC Pocket 在列表标题旁放一行，还会自动显示更紧张的窗口（5 小时或一周）；T3 单独一个区，有每日图表（[`UsageDailyChart.tsx`](https://github.com/pingdotgg/t3code/blob/6577c6b6185167c0e502fa01f9e8e8f2dee6ff25/apps/mobile/src/features/usage/UsageDailyChart.tsx)）。
- **导航不统一。** T3 和 coucou 有分区或标签，Paseo 和 codex-relay 用抽屉，CC Pocket 在手机上不用标签。

## 3. Mooring 没有、别家有的

| 功能 | 谁有 | Mooring |
|---|---|---|
| 推送 | Claude、Cursor，及上一轮查过的 coucou、Happy、Paseo | 没有 |
| 首页搜索 | T3、coucou、CC Pocket | 没有 |
| 额度放在列表上 | CC Pocket | 要进额度页 |
| 划掉后撤销 | T3 | 没有（`mobile/src` 里搜不到 undo 或撤销） |
| 置顶会话 | T3、CC Pocket | 没有 |
| 审 PR、合并 | Cursor；T3 有 Pulls 区；Paseo 显示 PR 状态 | 只能看改动 |
| 今天的工作统计 | coucou | 没有 |
| 首页直接写任务 | Happy | 要点「+」进新页面 |

Mooring 已有、和主流一致的：首页直接批准，审批固定在最上，已处理和稍后提醒，未发送，按空间筛选。

## 4. 关键事实核验

| 事实 | 影响哪条结论 | 来源与日期 | 结果 |
|---|---|---|---|
| 没有一家把首页做成数字概览 | 方向 A 的数字格要不要留 | 反查：读了 T3、coucou、CC Pocket、Happy、Paseo、Nimbalyst、codex-relay 的首页或导航源码和截图；2026-11-07 | 在这 7 家里成立；Claude、Codex、Cursor 的布局文档没写，无法确认 |
| T3 有单独的用量区和每日图表 | 之前我说「图表不加」 | 固定 commit 源码 | 成立：别家有，只是不放首页 |
| coucou 首页有今天统计 | 同上 | `HomeView.swift` 固定 commit | 成立 |
| Claude App 不能选 Bypass 权限 | Mooring 手机能切全部放行，和官方相反 | [官方文档](https://code.claude.com/docs/en/mobile) Limitations 节；2026-11-07 | 成立 |
| Mooring 首页没有搜索 | 建议加搜索 | `git grep` `origin/main` 的 `mobile/src`：只有 dirs、tools 有 | 成立 |
| CC Pocket 手机上没有底部标签 | 导航不统一 | 只看了宣传截图；源码只查了 `workspace_shell_screen.dart`，没读完 | 没完全核实，只代表截图所示的首页 |

## 5. 覆盖范围和没做到

候选约 25 个（上一轮 20 个，这轮新增 CC Pocket、Cursor iOS、Codync、SeaWork、Remote Codetrol）。首页深挖 10 个：6 个读了固定版本源码或截图（T3、coucou、CC Pocket、Happy、Paseo、Nimbalyst），codex-relay 只读了导航文件，3 个只读官方文档（Claude、ChatGPT Codex、Cursor）。

没做到：没有安装任何竞品；没有看应用商店评论和 Reddit、X 上的用户声音，这轮没有用户怎么说的证据；Codync、SeaWork、Remote Codetrol 只看了搜索结果或 README；宣传截图可能和真实使用不同。这份报告没有经过第三方核对，由我自己核了上面的关键事实。
