---
status: accepted
date: 2026-11-09
decision-makers: 用户
---

# 对外改名 Fibo；内部名字和 `MOORING_*` 不动

## Context

推广前查名时发现 mooring.sh 是同类产品（coding agent 工具），继续叫 Mooring 会和它混在一起。用户挑了 Fibo（斐波那契的兔子：一对生两对，agent 一个接一个冒出来；读「菲波」），官网先用 fibo.page。已知同名：fibo.dev（敏捷团队的估时投票工具）、Fibo Labs（fibo.io）、FIBO 金融本体标准、FIBO 健身展；USPTO 里活着的 FIBO 商标在建材、光纤、光纤检测仪器，没有软件开发工具。

已经装着的 Mooring 有：开着的终端（环境里只有 `MOORING_*`，`MOORING_HOOK` 指向 `.../Mooring.app/Contents/MacOS/mooring`）、`~/.config/mooring`、`~/.local/share/mooring-workbench` 里在跑的后台、Claude 设置里测 `$MOORING_HOOK` 的 hook 条目、PATH 上的 `mooring` 链接、包标识 `local.mooring.workbench`。

## Decision

用户看得到的都叫 Fibo：App 名和 Info.plist、菜单、窗口、欢迎页、设置、通知、报错、英文表、命令帮助和给 Agent 的说明（`fibo todo add`、`fibo --skill`）、发布资产 `Fibo-X.Y.Z-<arch>.zip/.dmg`、DMG 卷名、Homebrew cask `fibo`、包标识 `page.fibo.app`、配置目录 `~/.config/fibo`、新装的运行目录 `~/.local/share/fibo-workbench`、日志和更新缓存的 `Fibo` 目录、README。

**不做**：

- 不改 crate、模块和程序文件名。包里的程序还是 `mooring`，`fibo`（和以前的 `keepline`、`keeplined`）是指向它的链接：老终端的 `MOORING_HOOK` 路径和代码里找 `mooring` 的地方都不用动。开发构建另有一个小程序 `fibo`，转给旁边的 `mooring`。
- 不把 `MOORING_*` 换成 `FIBO_*`。hook 条目仍然测 `$MOORING_HOOK`：老后台开的终端在后台交接后还活着，环境里只有 `MOORING_*`，换成 `$FIBO_HOOK` 会让这些终端的 hook 悄悄失效。程序拿到的环境另外带上 `FIBO_*`（和 `KEEPLINE_*` 一样由 `legacy::with_old_names` 加），启动时 `FIBO_*` 也会被收进 `MOORING_*`。
- 不改手机端连接的加密字符串、中继路径 `/keepline`、报告标签 `<fibo from=…>`、插件清单里的 `"mooring"` 级别字段、签名过的官方插件目录、`~/.local/share/mooring/sources`、Codex skill 目录 `~/.codex/skills/mooring`、GitHub 仓库名。
- 这次不改手机 App 和 Windows 安装包的名字（要真机和 Windows 机器验）。

## Consequences

- 升级：配置 `mooring` → `fibo` 搬一次、原地留链接（Keepline 时期留下的 `keepline` 链接经 `mooring` 链接仍然能到）；已有的 `mooring-workbench` 继续当运行目录，后台照常交接终端。
- 更新器原地换包，所以装在 `Mooring.app` 的会一直叫这个文件夹名（显示名是 Fibo）。拖一个新的 `Fibo.app` 进来、删掉旧的，老终端里的 `MOORING_HOOK` 路径就断了，要重开那些终端。
- 包标识变了，macOS 的通知授权和文件夹访问授权要重新给一次。下载的新包是旧标识也接受（团队 id 说明是谁签的），App 自己认新旧两个标识。
- 旧的 `<!-- 由 Mooring 写入` / `<!-- 由 Mooring 生成` 标记仍算我们的，启动时照常更新成新文字。

## Implementation Plan

- `crates/fibo-core/src/legacy.rs`：`other_names`、`with_old_names` 加 `FIBO_*`，`adopt_old_env` 先收 `FIBO_*` 再收 `KEEPLINE_*`，`move_old_config` 依次搬 `mooring`、`keepline`，`runtime_dir` 先找 `fibo-workbench`，再找 `mooring-workbench`、`keepline-workbench`。
- `crates/fibo-core/src/cli_entry.rs`：PATH 上放 `fibo` 和 `mooring` 两个链接。
- `crates/fibo-core/src/update/`：`BUNDLE_ID`、`OLD_BUNDLE_ID`、资产名、cask。
- `package-macos.py`、`scripts/release-macos.py`：Fibo.app、`fibo` 链接、包标识、版本号取自 crate。
- 避免：在代码里把裸的 `"mooring"` 换成 `"fibo"`，它是程序文件名；按 `mooring serve` 找后台进程的地方要同时认 `fibo serve`（`main.rs` 的 pkill）。

## Verification

```sh
cargo test -p fibo-core legacy:: update:: cli_entry::
cargo test -p fibo-host cli_setup:: pty::
python3 package-macos.py --sign-identity - --output target/bundle/Fibo.app
target/bundle/Fibo.app/Contents/MacOS/fibo --version
target/bundle/Fibo.app/Contents/MacOS/mooring --version
```

## Alternatives

- 全部改名（crate、`FIBO_*` 为主、程序文件 `fibo`）：老终端的 hook 和 `MOORING_HOOK` 路径会断，diff 大、收益只在源码里。
- 继续叫 Mooring：和 mooring.sh 同类撞名。
- Fibonaut：全后缀都空，但用户选了更短的 Fibo。

## More Information

- 用户原话：「我觉得 Fibo 非常不错。」「先用 fibo.page 吧，如果不错我再买 fibo.sh」。
- 重开条件：Fibo 遇到商标异议，或用户买了 fibo.sh 要换主域名（只改 README 和官网链接）。

## 2026-11-13 追加

「不改 crate、模块和程序文件名」被 [2026-11-13-rename-files-fibo.md](2026-11-13-rename-files-fibo.md) 推翻：用户要求文件名也改。`MOORING_*` 等兼容项仍不动。
