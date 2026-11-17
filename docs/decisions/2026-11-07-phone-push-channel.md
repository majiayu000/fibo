---
status: proposed
date: 2026-11-07
decision-makers: majiayu000
---

# 手机提醒先走前台服务加现有加密连接，不接 FCM 和厂商推送

## Context and Problem Statement

手机只有开着 App 才知道有事要处理。进度页写着「需要 FCM；国内安卓可能还要接厂商推送，你定账号」。用户的手机是 OPPO Find X9 Ultra，国行 ROM（PMA110_16.0.10.501(CN01)，ColorOS V16.1.0）。

要定的是：走哪条通道，用户要提供什么。

## Decision

先做不要账号的一条：Android 上开「后台保持连接」后，前台服务（`remoteMessaging`）让进程和现有的 Noise 连接活着，手机根据解密后的卡片在本机发通知。中继和协议都不变。默认关，要用户自己打开。

- 什么算要提醒：只看卡片的 `needs_you`（电脑上 `status::needs_you` 算的）。手机只做「新出现才响一次」和设置页的开关，不写第二份定义。
- 通知里不放审批的命令；开了 App 锁时连会话标题也不放；锁屏用 `VISIBILITY_PRIVATE`。

不做：FCM、OPPO 推送、聚合 SDK、中继代发。不写厂商无关的中继接口：这条路不用它，以后真要接再按实际通道定。

证据（2026-11-07）：

| 通道 | 结论 | 依据 |
|---|---|---|
| FCM | 这台手机上否定 | 实测：手机上有 GMS，但 `GcmService` 显示 Not connected；13:20–17:09 尝试 16 次全失败，每次十几毫秒内就报 `err:3`，没有一次连上。同一 Wi‑Fi 下 `adb shell nc` 能连 `mtalk.google.com` 的 5228 和 443，所以不像网络问题。`err:3` 的含义没有文档，「ColorOS 拦了 GMS 联网」是推断（社区模块 fcmfix-coloros 也这么说，且要 root）。 |
| OPPO 推送 | 个人拿不到 | 官方：要企业开发者账号并上架；个人开发者的权益里没有 Android 应用；上架要软著；测试权限 1000 条/天、单设备 2 条/天、不支持通讯与服务类；不上架的通道只给政府类、企业内部通讯办公类。 |
| 聚合 SDK（极光、个推等） | 绕不开 OPPO | 官方：OPPO 通道的参数要自己去 OPPO 平台申请，极光文档写明要企业账号和上架。 |

## Consequences

* Good, because 不要账号、不要备案、不动中继，内容仍只在两端。
* Bad, because 通知栏常驻一条；`HeadlessJsTaskService` 全程持有 partial wake lock，耗电没量过。
* Bad, because 前台服务本身挡不住 ColorOS：实测切后台 5–11 秒后 `OAppNetControlService` 以 `App bg(IMMEDIATELY)` 关掉连接并把 UID 加进黑名单，之后不重连，Android 自身的网络策略却显示未拦（`effective=NONE`）。用 `adb shell dumpsys deviceidle whitelist +包名` 加白名单后不再断（三段，最长约 11 分钟），撤掉又断。但同样显示在白名单里、由系统询问框加入时，三次都在切后台约 6–16 秒后断开，换新进程、不碰开关也一样；再用 adb 撤掉重加又不断。原因没查明，只有这一轮序列，也可能有别的未控制因素。开开关时的询问框已加上，但目前不能认为它够用。
* Bad, because 进程被系统杀掉就没有提醒。系统带回来时可能不许进前台（已捕获），要等下次打开 App。开机不会自动启动。
* Bad, because 中继会看到这条连接全天在线、有事件时流量变化。内容仍看不到，但时间信息比以前多。
* Bad, because 电脑睡着或断网时没有提醒。任何通道都一样，判断在电脑上。
* iOS 不适用：没有前台服务，只能走 APNs，要开发者账号。

## Implementation Plan

* **Affected paths**: `mobile/src/remote/alerts.ts`（规则，纯函数）、`mobile/src/remote/background.ts`（接线）、`mobile/modules/mooring-alerts/`（Android 服务和通知）、`mobile/src/app/settings.tsx`。
* **Patterns to follow**: 状态词走 `statusWord`，标题走 `cardTitle`，是否要提醒走 `needs_you`。
* **Patterns to avoid**: 不要用 `dataSync`（Android 15 起 24 小时内限 6 小时）；不要把提醒判断搬到中继；不要把审批命令放进通知。

### Verification

- [ ] `cd mobile && bun test test/alerts.test.ts`
- [ ] 真手机三天试用：屏幕锁着时，提醒在 60 秒内到达的比例；`dumpsys batterystats --charged dev.mooring.mobile` 的耗电。

### 放弃条件

- 到达率 ≥ 95%、耗电 ≤ 5%/天：采用，把状态改成 accepted。
- 到达率够，耗电 5%–10%/天或更高：去掉 wake lock，或改成原生的轻量连接（中继加一种不带内容的唤醒帧）。
- 打开系统放行后到达率仍 < 80%：前台服务否定。用户有公司主体就走 OPPO 推送；没有就不做推送，进度页写明。

## Alternatives

* FCM：免费，但这台手机上连不上，非 root 修不了。别的手机可能可用，等真有这样的用户再说。
* OPPO 推送：唯一明确承诺进程不在也能到的通道，代价是公司主体、软著、备案、上架。
* 聚合 SDK：多一层依赖和一家能看到元数据的公司，OPPO 那些条件一样不少。
* 原生轻量连接加中继唤醒帧：更省电，但要 Kotlin 重写一部分连接并部署中继；等量出电费再说。
* UnifiedPush / ntfy：多装一个常驻 App，ColorOS 杀进程的问题还在。

## More Information

重开条件：用户有了公司主体；或手机换成能连上 FCM 的；或要支持 iPhone。详细比较和官方文档链接见 `~/Documents/claude-pages/mooring/2026-11-07-phone-push/index.html`。

## 2026-11-16 移植到 Fibo

11-07 的分支没合入；11-16 按原意移植到当时的 main（分支 `feat/phone-alerts-v2`），上面的结论不变，真机三天试用和放弃条件照旧，还没开始。和 11-07 不同的地方：

* 路径改名：`mobile/modules/fibo-alerts/`（Kotlin 包 `page.fibo.alerts`、原生模块 `FiboAlerts`、后台任务 `FiboKeepConnected`）。手机包名 `dev.mooring.mobile`、存储键 `mooring.settings` / `mooring.alerts`、通知渠道 id `mooring-asks` / `mooring-service` 不改：都已经在手机上。
* 开关的存储字段从 `keepConnected` 改成 `backgroundAlerts`，只认 `true`。11-07 试用包存下的 `keepConnected: true` 不再生效，换上新包后要在设置页重新打开；没有这个字段的旧设置读成关。规则在 `keepsConnected`（`mobile/src/remote/alerts.ts`），有测试。
* 通知的字跟设置里选的语言（`tr()`），后台没有窗口时也一样。通知栏常驻那一条和两个渠道名在原生层，跟手机系统语言（`res/values`、`res/values-zh`），不跟 App 里选的语言。
* 状态栏小图标换成 Fibo 标志的 f 和点。
