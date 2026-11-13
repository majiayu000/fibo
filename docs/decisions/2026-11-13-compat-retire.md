# 旧名字的兼容怎么退：Keepline 这次删，Mooring 分步退

- 日期：2026-11-13
- 状态：采用
- 相关：2026-11-09-rename-fibo.md、2026-11-13-rename-files-fibo.md

## Context

文件和程序改名 Fibo 后，用户问「你觉得需要兼容吗」。判断标准：旧名字写进了用户机器上的文件（Claude 的 settings.json、正在跑的终端环境、手机上的安装、已发布的插件）就得留，只在我们代码里的随时能删。

## Decision

- Keepline 那一代从没公开发布，只有一台机器用过。11-13 用 `ps -Eww` 查了那台机器全部 803 个进程：没有只带 `KEEPLINE_*` 的，带 `MOORING_HOOK` 的 48 个也都带 `FIBO_HOOK`。所以删掉 `KEEPLINE_*` 变量（启动时不再认、终端和插件里不再设）、包里的 `keepline`/`keeplined` 链接、`~/.config/keepline` 的搬迁和 claude-quota 对 Keepline 包装的接管。
- 留下 Keepline 时期仍被数据用着的：`~/.local/share/keepline-workbench` 继续当运行目录（那台机器的 host 正在用）；测 `$KEEPLINE_HOOK` 的 hook 条目仍算我们的（标过期，安装时替换，不会另加一份）。
- 新装的 hook 写 `${FIBO_HOOK:-$MOORING_HOOK}`。v0.1.0–0.1.2 写的 `${MOORING_HOOK:-$KEEPLINE_HOOK}` 照样能跑，只是显示「已过期」，用户点一下才换（hook 不悄悄重装）。
- `MOORING_*` 继续设、继续是代码读的名字。等大多数装着的 hook 换成新写法后，再另起一次决定停掉它。
- 不动：手机包名 `dev.mooring.mobile`、`mooring://`、插件的 `"mooring"` 级别字段、包里的 `mooring` 链接、手机加密串和中继路径 `/keepline`。

## Consequences

- 升级后所有装过 hook 的人会看到一次「hook 已过期」，点更新即可；不点也不影响状态识别。
- 本机 `~/.config/keepline` 这个链接不再有人维护，留着无害，可手动删。

## Verification

- `crates/fibo-host/src/agents/hooks.rs` 的 `entries_written_under_the_old_name_are_ours_and_replaced`（两种旧写法都被认出并替换）、`a_terminal_from_before_the_rename_still_reports`（只有 `MOORING_HOOK` 的终端仍上报）。
- `crates/fibo-core/src/legacy.rs` 的 `fibo_names_are_adopted_and_mooring_ones_still_count`、`a_host_from_before_keeps_its_runtime`。

## Alternatives

- 全留：Keepline 的那部分没有任何用户，只是多一套名字要测要读。
- 连 `MOORING_*` 一起删：已装的 hook 会全部失效，状态识别坏掉，直到用户手动重装。
