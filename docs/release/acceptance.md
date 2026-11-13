# 同包跨端验收

**要支撑的决定：** 某个 commit 能不能发成下一个版本，并说「电脑和安卓都验过」。
**放弃条件：** 走到阻断项「不过」就停，修完出下一个候选版（RC）从头走；不在旧 RC 上补着走。iOS、Windows、Linux 不在这张表的门槛里，各自单列。

## 0. 钉住候选版

一次验收只认一个 commit。走查期间不换 App、不更新插件；中途合进来的 PR 归下一个 RC。

记录写在 `docs/release/<版本>-rc<N>.md`：commit、`receipt.json` 里两个二进制的 SHA256、DMG 的 SHA256（发版包才有）、APK 的 versionCode 和 SHA256、走查人和日期，以及下面每一项的结果。

结果只有四种：**过**（写亲眼看到的证据）、**不过**（写现象，阻断的标「阻断」）、**跳过**（写为什么）、**没验**（没走到的都算这里，汇报时原样列出）。

## 1. 桌面：状态、审批、恢复

按事件列，不按页面列。每一条 Agent 相关的都要走 Claude、Codex、OpenCode 三家（兄弟规则，`docs/lessons.md`），哪家没走写进没验。

方式：**命令** 用 `fibo list --json / events / read / explain` 看得出；**窗口** 要看界面；**有影响** 会动到正在用的会话，要用户同意或在空闲时走。

| 编号 | 类别 | 怎么走 | 看什么 | 方式 |
|---|---|---|---|---|
| `st.new` | 状态 | `fibo run <agent> "echo walk"` 开一个测试会话 | 侧栏出现、`status` 依次 working → review（等你验收），标题是任务不是路径 | 命令 |
| `st.done` | 状态 | 在窗口里把它标记完成 | 变「做完了」、无状态色；Agent 不能自己标 | 窗口 |
| `st.esc` | 状态 | 让它前台跑 `ping -c 60 127.0.0.1`，按 Esc | 约 10 秒内不再是在跑，不卡在 working | 命令 |
| `st.bgshell` | 状态 | 让它起一个后台任务（脚本）和一个监听固定端口的服务 | 任务算在跑；服务算等你验收加小字；`fibo explain` 说明依据 | 命令 |
| `st.subagent` | 状态 | 让 Claude 开一个子代理做 30 秒的事 | 父会话在跑，子代理做完后父会话不残留在跑 | 命令 |
| `ap.allow` | 审批 | 权限换逐个确认，让它 `touch walk-approved.txt`，在电脑上点允许 | `events` 有 approval → approval_gone；文件出现 | 窗口 |
| `ap.deny` | 审批 | 再要一次，电脑上拒绝并说一句理由 | 理由到了 Agent 那边 | 窗口 |
| `ap.question` | 提问 | 让它用提问框问三选一 | 状态是等你回答，`fibo prompt` 会拒绝（不加 `--force`）；答完它收到 | 命令+窗口 |
| `ap.plan` | 计划 | 计划模式出计划 → 批准 | 卡片显示计划，批准后接着做 | 窗口 |
| `ap.phone` | 审批 | 同 `ap.allow`，在手机上点 | 电脑上的卡片同时消失 | 手机 |
| `rc.handoff` | 恢复 | 换 App（后台交接，`serve --takeover`）| 换前记下每个 Agent 的 PID 和状态；换后 PID 不变、状态一样、在跑的一轮做完能变等你验收 | 有影响 |
| `rc.hostrestart` | 恢复 | 后台进程被杀后重新起来 | 终端还在或明确显示断开；前 45 秒内不把在跑判成要你处理 | 有影响 |
| `rc.plugin` | 恢复 | 后台起来之后再更新一个插件 | 新插件生效，不用重启 | 命令 |
| `rc.resume` | 恢复 | 关掉测试会话的标签，从侧栏恢复 | 只开一个标签、一个进程；目录和身份对；不出一排 zsh | 窗口 |
| `rc.continued` | 恢复 | Claude 转后台接着跑（continued-in） | 终端跟到新会话，不卡在跑 | 命令 |
| `rc.window` | 恢复 | 关掉窗口再打开 | 标签、分屏、侧栏和关之前一样；没有重复标签 | 窗口 |
| `rc.oldfiles` | 恢复 | 用上一版写过的运行目录直接起这个 RC | 设置、主题、待办、配对的手机都还在；读不出的文件不被覆盖成空 | 有影响 |

## 2. 手机

用这个 RC 的 APK 把 [`phone-app/walkthrough.md`](../phone-app/walkthrough.md) 整张走一遍（`phone-walk` skill），报 过 / 不过 / 跳过 / 没验 各几项。`handoff` 那一项接在 `rc.handoff` 后面走。

## 3. 干净的 Mac 安装

在本机新建一个 macOS 标准用户（没有 `~/.config/fibo`、没装过 hook、没有旧运行目录），用官网写的入口装：

| 编号 | 怎么走 | 看什么 |
|---|---|---|
| `in.cask` | `brew install --cask` | 装得上；`fibo` 和 `mooring` 都在 PATH |
| `in.dmg` | 下载 DMG，拖进应用程序 | Gatekeeper 不拦；`spctl -a -vv` 显示已公证 |
| `in.first` | 第一次打开 | 后台起来；hook 装进 Claude 设置；没有报错弹窗 |
| `in.agent` | 在里面开一个真的 Claude 做一件事 | 被认出、状态会变、侧栏标题对 |
| `in.pair` | 配一次手机 | 能连上，能看到这个会话 |
| `in.update` | 装上一版，再用 App 自己的更新升到 RC | 升级后数据还在 |

这只验「这台 Mac 上一个新账户」。别的系统版本、Intel 机器、从没装过 Xcode 命令行工具的机器，写进没验。

## 收尾

汇总四种结果的数目，每个「不过」单列一行，写出决定：发 / 出下一个 RC / 待验证。只有真机才发现的问题加到 [`real-device-findings.md`](../phone-app/real-device-findings.md)。
