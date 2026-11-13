---
status: accepted
date: 2026-11-08
decision-makers: 用户（要应用内更新）、Claude（做法）
---

# 发布版从 GitHub Releases 自己更新，换包靠改名，终端靠后台交接

## Context

第一个公开版 v0.1.0 要发了，项目每天在变；没有应用内更新，用户会一直停在旧版。发布约定：tag `vX.Y.Z`，资产 `Mooring-X.Y.Z-arm64.zip`（`ditto -c -k --keepParent` 打的、Developer ID 签名并公证的 Mooring.app）、`.dmg` 和 `SHA256SUMS`，团队 `C5UWZ934C2`，包标识 `local.mooring.workbench`。

## Decision

- **查**：只有从 `.app` 包里跑、包标识对的才查和装；`target/` 下的开发版永远不查（`update::packaged_bundle`）。每天最多一次（启动约一分钟后第一次），外加「设置 › 关于 › 检查更新」。开关「自动检查更新」默认开，存在 `~/.config/mooring/update-check`；文件读不了当作没开，不联网。自动检查失败不出声，手点才报错。
- **装**：zip 下到 `~/Library/Caches/Mooring/updates/<版本>/`，对 `SHA256SUMS`；解到正在运行的包旁边的隐藏目录 `.Mooring.app.update/`；在那里查包标识、版本、`codesign --verify --deep --strict` 加 Developer ID 且 OU=团队的要求、Team ID、`spctl --assess`；全过才改名换包（旧包先改名成 `.Mooring.app.retired`，新包放进原位，失败就改回去），再删旧包。任何一步失败都清掉解出来的和下载的，原 App 一字不动。
- **重启**：换包后用 `start_after_exit` 等本进程退出再启动新包里的 `fibo-app`（带原参数），本进程退出。新 App 的 `ensure_host` 发现后台跑的是旧代码且能交接，就起新后台 `--takeover` 接走全部终端——和手动换包后重开 App 走的是同一条路，不另写交接。
- **不装的情况**：Homebrew 装的（包在 Caskroom 里或有 `mooring` cask）只提示 `brew upgrade --cask mooring`，按钮拷贝命令；包不属于当前用户或目录写不了、发布里没有这台 Mac 架构的 zip 时，按钮改成打开发布页。
- **界面**：状态栏右侧在有新版本时多一项「新版本 X · 更新」，点了就装；装不了的点开「设置 › 关于」看原因。侧栏不动。
- **联网**：这次核对了代码，App 自己发出的网络请求只有这一个检查；手机中继要开了才连，远程插件目录要点了才取，webhook、SSH 电脑、装的插件都是用户自己加的。README 常见问题写了这句。

## Consequences

- 每个公开版都必须带这份更新代码，否则用户卡在它上面；v0.1.0 起就有。
- 新版要是升了协议大版本，新 App 连旧后台会走现有的「后台版本过旧」提示（和手动换包一样），不是更新器的事。
- 自动检查和安装时 App 在后台、屏幕关着会被系统限速：本机 debug 包实测启动后 2–4.5 分钟才发出第一次检查，45 MB 的 SHA-256 算了约 6 分钟。发布版是优化编译，前台点「更新」时没有这个限速，没实测。

## Implementation Plan

- 规则和流程：`crates/fibo-core/src/update/`（`install.rs` 经 `Tools` 做每一步，测试换成假的）。
- 系统调用：`crates/fibo-platform/src/app_bundle/`（macOS 用 curl、ditto、plutil、codesign、spctl；其他系统 `SUPPORTED=false`）。
- App：`crates/fibo-app/src/update.rs`（全局状态、每日检查、安装、重启），`workbench/statusbar_update.rs`，`workbench/settings/updates.rs`。
- 只给本机假发布用的环境变量（真地址下全部不生效）：`MOORING_UPDATE_API`（换 API 地址并允许 http）、`MOORING_UPDATE_UNNOTARIZED_OK`（只放行 spctl 因「没公证」拒绝的）、`MOORING_UPDATE_AUTO_INSTALL`（查到就装，给锁屏的端到端测试）。
- 不要往正在运行的包里写文件，也不要在包外的另一个卷上解压再跨卷移动。

## Verification

- `cargo test -p fibo-core --lib update::`：版本比较、发布 JSON、资产选择、SHA256SUMS、各种校验失败原包不动、只读目录、Homebrew、开发版不更新（`dev_builds_never_update_themselves`）。
- 端到端：本机打两个签名包（0.1.0 和 0.2.0，host 二进制不同），本地 HTTP 假发布，隔离 HOME 和运行目录，开着终端从旧包更新，看终端进程号不变、新后台接手。

## Alternatives

- Sparkle：要引入 Objective-C 框架和 appcast 签名，现有发布约定已经有签名、公证和 SHA256SUMS，够用。
- 让 Homebrew 装的也自己更新：cask 的版本记录会和实际不一致，`brew outdated` 会一直说旧；等真有 cask 再看要不要标 `auto_updates true`。
- 在 App 里另写一遍后台交接：已有的启动流程会做，另写一份就有两处要一起改。

## More Information

重开条件：发布改用 dmg-only、或开始出 Intel 包、或 cask 上线。
