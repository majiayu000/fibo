---
status: accepted
date: 2026-11-13
decision-makers: 用户
---

# 文件、crate 和程序也改名 Fibo；旧名字留成链接

部分推翻 [2026-11-09 对外改名 Fibo](2026-11-09-rename-fibo.md) 的「内部名字不动」。

## Context

11-09 只改了用户看得见的名字，crate、模块和程序文件还叫 mooring。11-13 用户问「现在 mooring 的名字还有吗」「我是说文件名」，看了清单后说「都要改」。

## Decision

改成 Fibo 的：

- `crates/mooring-*` → `crates/fibo-*`，包名和 Rust 模块名（`mooring_core` → `fibo_core`）跟着改。
- 程序：`mooring` → `fibo`，`mooring-app` → `fibo-app`，`mooring-relay` → `fibo-relay`。开发构建不再有转发用的小 `fibo`。
- 图标 `assets/Fibo.*`、shell 集成 `fibo.bash` / `fibo.fish` / `fibo-integration`、`packaging/windows/fibo.iss`、Agent skill 目录 `skills/fibo`。
- 看得见的字：状态栏「后台 Fibo」、汇报标签 `<fibo from=…>`、XTVERSION `fibo <版本>`、中继登记的 `ver`。
- 注释、帮助、脚本、CI 里的 `mooring <子命令>` 写成 `fibo <子命令>`。

**不改**（改了会弄坏已有的安装）：

- `MOORING_*` 变量：老终端里只有它，hook 也按它判断。
- 插件 `plugin.json` 里的 `"mooring"` 级别字段：已发布插件都这么写。
- `mooring://` 配对链接和手机包名 `dev.mooring.mobile`：改包名在安卓上是另一个 App，配对和手机里存的东西都丢。
- 手机存储键 `mooring.*`、钥匙串里的 `mooring.plugin.*`：改了读不到旧值。
- GPUI 动作命名空间 `mooring::…`：用户的快捷键文件里写的是它。
- `~/.config/mooring`（已是指向 `~/.config/fibo` 的链接）、运行目录 `keepline-workbench` / `mooring-workbench`、旧 bundle id、旧 cask 名。
- `docs/decisions` 里的历史记录照原文。

旧名字留成链接：App 包里 `mooring` → `fibo`、`mooring-app` → `fibo-app`（还有 `keepline`、`keeplined`）；PATH 上 `fibo` 和 `mooring` 都指向包里的 `fibo`；Windows 把 `fibo.exe` 复制成 `mooring.exe`；中继镜像里 `mooring-relay` 链到 `fibo-relay`。新 skill 写进 `skills/fibo` 时，删掉旧的 `skills/mooring`（只删我们写的）。

## Consequences

- 老终端的 `MOORING_HOOK=.../MacOS/mooring` 照样能跑；找后台进程的地方认 `mooring serve` 和 `fibo serve` 两种。
- 别的会话手上的分支会和这次改名冲突，要 rebase（git 能认出改名，多数自动合上）。
- 活动监视器里看到的是 `fibo` 和 `fibo-app`。

## Implementation Plan

- 受影响：整个 `crates/`、`package-macos.py`、`packaging/windows/`、`.github/workflows/`、`scripts/`、`eval/`、`ci-test.sh`、`run.sh`、基线文件里的路径。
- 要避免：把上面「不改」的东西当成漏网之鱼顺手改掉。

## Verification

- `cargo build --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`、`sh ci-test.sh`
- `crates/fibo-host/src/cli_setup.rs` 的 `the_skill_written_under_the_old_name_goes_unless_someone_else_wrote_it`
- 换 App 后：`ls -l .../Fibo.app/Contents/MacOS` 里 `mooring` 是链接；老终端里 `$MOORING_HOOK` 能跑；`fibo list` 终端数不变。

## Alternatives

- 继续只改看得见的（11-09 的做法）：用户明确要文件名也改。
- 连 `MOORING_*`、手机包名、配对链接一起改：会让已经开着的终端和已配对的手机失效，收益只是名字。
