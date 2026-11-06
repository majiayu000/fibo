# 拆分超 800 行的文件

2026-11-05 记录。基线提交 `eedc95a`（审计修复合入之后）。批次 4 之前的 G1 见 [spec.md](spec.md)，G1 合入后验收基线要重新测一遍。

**要支撑的决定**：能否把 `file-size-baseline.txt` 清空，且所有功能和拆分前一致。

**指标**：30 个文件、49,706 行全部降到 800 行以内；基线文件为空；下面「验收」五层全部通过，第 6 层由人走查后无差异。

**放弃条件**：某个文件做不到只搬不改（必须改逻辑才能拆开），或拆后 `perf-gate` 超预算且查不出原因。这时撤回该文件的拆分，留在基线里并写明原因，不为凑行数改行为。

## 范围

只管 `crates/mooring-core/tests/file_size.rs` 认定的源文件，即 `file-size-baseline.txt` 的 30 项。

不在范围内：`mobile/bun.lock`（生成）、`patches/gpui-retained-backbuffer.patch`（生成）、`docs/multi-machine/sidebar-directions.html`、`docs/phone-app/design.html`（设计稿，不是源文件）。

另有 17 个源文件已在 700–800 行（如 `workbench/settings/theme_editor.rs` 797、`workbench/statusbar.rs` 792、`workbench/tests.rs` 785）。拆出来的代码不要塞进这些文件，建新文件；新文件控制在 600 行左右，不要刚好卡在 799。

## 怎么拆

- **只搬不改**：拆分提交里只允许移动代码，以及为此必须的 `mod`、`use`、可见性调整。不改名、不改签名、不顺手优化。发现 bug 另开提交。
- **一个提交只拆一个文件**，标题写 `refactor(split): <文件路径>`。验收按这个前缀逐个审计，所以 main 同时有别的提交也不影响。
- **测试和被测不同时动**：批次 2 靠没动过的 `managed_session.rs` 把关，批次 3 再拆它，此时产品代码不动。`harness.rs`、`visual.rs` 是看画面的工具，放到最后。
- **用 worktree 做，每批尽快合入 main**：这些文件几乎每天都有别的会话在改，分支拖久了会不断冲突。
- 每拆完一个文件，运行 `MOORING_GUARD_UPDATE=1 cargo test -p mooring-core --test file_size` 降低基线，和拆分一起提交。

## 文件清单

总行数、测试模块行数按基线提交统计。「其余」就是把测试模块搬走后还剩多少。只有 4 个文件搬完测试就能达标，其余 26 个要拆产品代码或测试文件本身。拆完把状态改成提交号。

### 批次 1 测试外移（只搬测试模块）

| 文件 | 总行数 | 测试模块 | 其余 | 切法 | 状态 |
|---|---:|---:|---:|---|---|
| `core/src/content.rs` | 1023 | 353 | 670 | 测试外移后即达标 | 待拆 |
| `app/src/ui/theme.rs` | 904 | 160 | 744 | 测试外移后即达标。**有陷阱 5** | 待拆 |
| `app/src/desktop.rs` | 806 | 327 | 479 | 测试外移后即达标。**有陷阱 2** | 待拆 |
| `app/src/workbench/board_drawer.rs` | 830 | 39 | 791 | 测试外移后 791 行，余量太小，顺手把 `drawer_view` 搬走 | 待拆 |

### 批次 2 后台与核心（产品代码，靠未动的集成测试把关）

| 文件 | 总行数 | 测试模块 | 其余 | 切法 | 状态 |
|---|---:|---:|---:|---|---|
| `core/src/model.rs` | 1901 | 802 | 1099 | 测试外移；`Workspace` 方法按读 / 改（`edit`）分开，`Edit` 枚举单列 | 待拆 |
| `host/src/content/mod.rs` | 2055 | 1149 | 906 | 测试外移（1149 行）；`Parser` 的 `result` 搬走 | 待拆 |
| `host/src/recognition.rs` | 1791 | 978 | 813 | 测试外移后 813 行；`processes` 及其辅助函数单列 | 待拆 |
| `host/src/log_index.rs` | 2063 | 739 | 1324 | 测试外移；`update_unlocked`、`Recovery`、`Elide` 各自成文件。**有陷阱 3** | 待拆 |
| `host/src/control.rs` | 1247 | – | 1247 | `Args` 解析、`open`、`explain`、`run_agent` 按命令分 | 待拆 |
| `host/src/host/mod.rs` | 1100 | – | 1100 | `serve_runtime`、`take_over`、`start_deadlines` 搬走（`host/` 已是目录） | 待拆 |
| `host/src/relay.rs` | 939 | – | 939 | `connect`（291 行）、`Bridge`、`Live` 分开 | 待拆 |

### 批次 3 后台测试文件（产品代码不动）

| 文件 | 总行数 | 测试模块 | 其余 | 切法 | 状态 |
|---|---:|---:|---:|---|---|
| `host/tests/managed_session.rs` | 6632 | – | 6632 | 按主题分子模块（手机中继 / 远程 / 事件流 / CLI 分屏 / 所有权等）。根文件必须留在原路径。**有陷阱 1** | 待拆 |
| `host/tests/pty_stall.rs` | 904 | – | 904 | 辅助类型（`Conversation`、`Stamps`、`watch`）和两个用例分开 | 待拆 |

### 批次 4 窗口界面（先读 docs/performance/index.html）

| 文件 | 总行数 | 测试模块 | 其余 | 切法 | 状态 |
|---|---:|---:|---:|---|---|
| `app/src/terminal/screen.rs` | 1329 | – | 1329 | `Capture`、`AsciiGlyphs`、绘制、原生几何校验分开。**有陷阱 5** | 待拆 |
| `app/src/terminal/pane.rs` | 2969 | 115 | 2854 | `Render`、`EntityInputHandler`、按键（`key`）、`ask_overlays` 分开。**有陷阱 2** | 待拆 |
| `app/src/workbench/mod.rs` | 1052 | – | 1052 | `launch_in_slot`、`sync_panes`、关闭流程搬走；`Workbench` 结构留下 | 待拆 |
| `app/src/workbench/render.rs` | 1479 | – | 1479 | 主 `render` 741 行不用切，其余 `impl Workbench` 块（`shortcut`、`empty_tab`、`tab_edge`）搬走 | 待拆 |
| `app/src/workbench/layout.rs` | 2119 | 8 | 2111 | `pane_view`（509）、`menu_view`、`turn_bar`、`confirm_close_view` 分开 | 待拆 |
| `app/src/workbench/sidebar.rs` | 1218 | – | 1218 | `sidebar_view`、`todos_view`、`space_menu_view` 分开 | 待拆 |
| `app/src/workbench/sidebar_list.rs` | 1269 | 34 | 1235 | `session_list`、`dense_row`、`session_row` 分开 | 待拆 |
| `app/src/workbench/board.rs` | 1149 | – | 1149 | `board_card`、`column`、`board_header` 分开 | 待拆 |
| `app/src/workbench/overview.rs` | 1051 | – | 1051 | `overview_view`、`session_card`、`todo_section` 分开 | 待拆 |
| `app/src/workbench/sessions.rs` | 836 | – | 836 | `here_card`（443）搬走 | 待拆 |
| `app/src/workbench/palette.rs` | 1274 | – | 1274 | 搜索逻辑和 `hit_row`、`hit_preview` 这类绘制分开 | 待拆 |
| `app/src/workbench/forms.rs` | 1098 | – | 1098 | `form_preview`、`form_criteria`、`form_card` 分开 | 待拆 |
| `app/src/workbench/task_detail.rs` | 1122 | 64 | 1058 | `detail_head`、`criterion_row`、`evidence_row` 分开 | 待拆 |
| `app/src/workbench/diagnostics.rs` | 2301 | 50 | 2251 | 内存 / CPU 排行、`scatter`、`DiagnosticView` 的 `Render` 分开 | 待拆 |
| `app/src/workbench/settings.rs` | 2104 | – | 2104 | 每个设置页一个文件（`settings/` 目录已有 `plugins.rs`、`theme_editor.rs`） | 待拆 |

### 批次 5 验收工具本身（放最后，产品代码不动）

| 文件 | 总行数 | 测试模块 | 其余 | 切法 | 状态 |
|---|---:|---:|---:|---|---|
| `app/src/workbench/visual.rs` | 1826 | – | 1826 | `visual_page`、`visual_dense_fixture` 分开，`visual_*` 按页面分。**有陷阱 3、4** | 待拆 |
| `app/src/harness.rs` | 3315 | – | 3315 | **全仓唯一不能纯搬动的文件**：`start` 一个函数 3255 行。按场景拆成 `*_evidence.rs`，沿用已有写法（`fn start(...) -> bool`）。**有陷阱 3、4** | 待拆 |

## 陷阱

前五条是拆分本身会踩的。标「不报错」的最危险：测试照样全绿，但保护已经没了。

| # | 位置 | 拆了会怎样 | 做法 |
|---|---|---|---|
| 1 | `ci-test.sh:37` 写死 `tests/managed_session.rs` | 拆成多个测试二进制或 `tests/managed_session/main.rs` 后，偶发失败不再重跑，CI 随机变红 | 已实测：cargo 的输出头变成 `Running tests/managed_session/main.rs`，脚本匹配不上。根文件留在原路径，用 `#[path = "managed_session/xxx.rs"] mod xxx;` 引入子模块 |
| 2 | `ui/theme.rs:850` 的颜色守卫写死 `terminal/pane.rs`、`desktop.rs` | **不报错**：搬走的代码不再被检查 | 拆 `pane.rs`、`desktop.rs` 时把新文件加进这个清单 |
| 3 | 按文件名判断的守卫：`tests/guard.rs:50,58`（`log_index.rs`、`harness.rs`、`visual.rs`）、`ui/tone.rs:73`、`tests/architecture_boundary.rs` 的 `FIXTURES` | 新文件被当成普通产品代码，测试报错。此时用 `MOORING_GUARD_UPDATE=1` 重写基线也能变绿，等于**把守卫放宽了** | 改守卫的路径规则，让新文件继承原有身份。重写 `guard-baseline.txt` 后，每个 `field` / `word` 的总数不得增加 |
| 4 | `harness.rs`、`visual.rs`、`pane.rs` 等的 `visual-test` 代码 | **不报错**：CI 的 clippy 和测试都不开这个特性，发布构建也不编译这部分 | 每次提交前都跑下面开 `visual-test` 的 clippy |
| 5 | `examples/paint_bench.rs`、`terminal_reference.rs` 用 `#[path]` 引入 `screen.rs`、`theme.rs` | **不报错**：`mod xxx;` 的解析目录变了，只有这两个 example 编不过。已实测：`theme.rs` 加一个普通 `mod` 后，CI 用的 clippy 仍然通过，开特性才报 E0583 | 这两个文件里的子模块一律写 `#[path]`（`screen.rs` 已是这样）；用特性检查确认 |

## 每个提交的自查（执行人）

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p mooring-app --all-targets --features visual-test -- -D warnings   # 陷阱 4、5
cargo test -p <改动的 crate>
cargo test -p mooring-core --test file_size --test guard
```

开 `visual-test` 的 clippy 原先有几个旧报错，所以这里和 CI 曾经只用 `cargo check`；报错修掉后两边都改成了 clippy。

另外遵守 `CLAUDE.md` 的提交说明格式（`Why:`、`Rejected:`）。提交被 hook 拦一次是正常的：读完它贴出的历史 `Rejected:` 再提交一次。

## 验收

全部拆完后做，在固定提交的独立 worktree 里跑：主工作区可能中途被别的会话提交（记录这份基线时就发生过，第一次结果因此作废）。终点提交和基线对比。

| 层 | 查什么 | 怎么查 | 证明力 |
|---|---|---|---|
| 1 纯搬动审计 | 每个 `refactor(split):` 提交没改逻辑 | 该提交删掉的每一行都要在别处原样新增，去掉空白、`mod`、`use`、可见性后比较；残留行逐条人工读 | 最强：覆盖没有测试的代码。脚本验收时写 |
| 2 测试名单 | 没有测试被搬丢 | 基线名单（`docs/file-split/baseline-tests.txt`）对比终点名单，去掉模块前缀后不得缺名 | 防止「全绿但少跑了」 |
| 3 全量测试 | 行为测试 | `sh ci-test.sh`、开 `visual-test` 的 app 测试、`mobile` 的 `bun test` 和 `bun run typecheck`、`eval/protocol-contract/check.py` | 只覆盖有测试的部分 |
| 4 性能和窗口 | 帧成本、主题场景 | `scripts/perf-gate.py`、`scripts/theme-scenes.py`、CI 的中继镜像任务 | 真窗口 |
| 5 画面对比 | 界面没变 | 基线两遍加终点各渲染全部页面、`MOORING_*_EVIDENCE` 场景和 `remote-*-evidence.sh`；差异按大小排好，**由人逐张看** | 只能筛图，不能判通过：彩排里纯搬动仍有约 3 成的图要看，故意改的界面也能筛出来 |
| 6 真机走查 | 自动手段够不到的 | 打包进真实 App，按下表最后一列走一遍，再跑被 `#[ignore]` 的真 Agent 用例（如 `real_claude_hooks_bind_at_startup_before_any_prompt`、`the_js_phone_client_chats_through_a_real_relay`）。**替换 App 需要先问用户** | 只有这层用真 Agent、真手机 |

通过标准：1、2 无差异；3、4 全过（基线已失败的另记）；5 筛出的图逐张看过，解释得了；6 无差异。任何一层不过，定位到具体 `refactor(split):` 提交，撤回或修复后重验。

## 验收命令

全程在同一台机器上跑，别的编译尽量停掉，屏幕不要锁。基线和终点必须是同一台，换机器就要重测基线。

**用 `apple` 验收，等拆分结束、它的终端释放之后再跑。** 2026-11-05 在两台机器上各跑两遍基线（`eedc95a`）：`SCROLLBACK`、`SELECTION`、`SELECTION_PIXEL`、`FOLLOW_INPUT`、`PEEK`、`PANE_DRAG`、`WHEEL` 这 7 个在本机不稳定，在 `apple` 上两遍都过，当时两边负载相近，所以怀疑是本机有人使用、窗口抢焦点，只有两遍数据，还没证实。`apple` 第二遍大面积失败是 PTY 用尽（`Device not configured`，526 个设备，上限 511），不是代码问题；脚本现在开跑前会检查，剩余不足 100 个就拒绝运行。那边的第一遍基线可用作参考，正式验收要在 PTY 释放后重测。

没有完整 Xcode 的机器也能跑：脚本发现没有 Xcode 时，只在验收用的 worktree 里给 GPUI 打开 `runtime_shaders`，不用 metal 编译器。这样编出来的着色器加载方式和正式版不同，所以这台机器上 `perf-gate` 对仓库基线的结论不算数，只比基线和终点两边的数字。

```sh
python3 scripts/split-check.py self-test
python3 scripts/split-check.py audit <基线> <终点>        # 第 1 层
# 第 2–5 层。按基线、终点、基线的顺序跑，三次都用终点的截图数据
python3 scripts/split-check.py measure <基线> <目录A> --fixture-from <终点>
python3 scripts/split-check.py measure <终点> <目录C> --fixture-from <终点>
python3 scripts/split-check.py measure <基线> <目录B> --fixture-from <终点>
python3 scripts/split-check.py report <目录A> <目录B> <目录C>
```

报告里「回归」要查；`compare.html` 里的图要人看；出问题时对可疑提交跑 `scripts/visual-check.py shots <提交> <目录>`，一个提交只拆一个文件，所以定位后直接撤回。

**这些场景在两台机器上都失败，等于没有覆盖**：`ROUTE`、`SPLIT`、`SHORTCUT`、`TAB`、`LINK`、`NEW_SPACE`、`OVERVIEW`、`PHONES`、`KEYMAP`、`SUBAGENT`。部分是缺前置数据或参数，原因没逐个查。它们对应的分屏、标签、快捷键、新空间、手机，第 6 层必须亲手过。页面 `CloseWorking` 所在那组也不稳定。其余场景只有基线两遍都过才算数。

**工具测过什么**：纯搬动判干净，改掉逻辑的提交被点名（`take` 改 `skip`），故意把「新建待办」改成「新建事项」时三张 `TodoNew` 被筛出。**没测过**：悬停、拖动中途这类截图到不了的状态；工具对 `harness.rs` 这种必须重构的文件只能标出来，要人读。

## 功能清单

「全部功能」以这张表为准，验收逐行打勾。发现漏项先补表。

| 功能 | 自动检查（层 3–5） | 真机走查（层 6） |
|---|---|---|
| 真实终端、关窗后继续跑、重启恢复 | `managed_session`、`RESTART` `RESUME` `RECONNECT` | 真 Agent 跑着时退出 App 再开 |
| 终端渲染、选择、滚动、滚轮 | `screen_fixtures`、`SELECTION` `SCROLLBACK` `WHEEL` `SCROLL_KEYS`、`eval/terminal-reference` | 看中文和 4K 深色 |
| 分屏、标签、拖窗格 | `SPLIT` `SPLIT_CLI` `TAB` `TAB_OVERFLOW` `PANE_DRAG` `LAYOUT` | 手动拖一次 |
| 空间 | `SPACE` `NEW_SPACE` `FOLDERLESS_SPACE` `TAB_CWD` `OSC7` | – |
| 会话状态、完成和放弃 | `one_owner`、`reasons`、`hook_shapes`、`COMPLETE` | 真 Claude / Codex 跑一轮，状态跟得上 |
| Agent 识别和 hook | `recognition`、`declared_agent`、`shell_integration`、`protocol-contract`、`SUBAGENT` | 真 Claude / Codex / Grok 各开一个 |
| 权限问答、发送文字 | `remote_typing`、`request_receipts`、`PINNED_ASK` | 真 Agent 弹一次权限 |
| 侧栏四种布局 | `SidebarDirectory` 等页面、`remote-sidebar-evidence.sh`、`perf-gate` | 手动切换 |
| 看板、鸟瞰 | `Overview` `Board*` 页面、`OVERVIEW`、`remote-board-evidence.sh` | 拖一张卡 |
| 待办表单和详情 | `TodoNew*` `TodoDetail*` `TodoStart` `QuickTask` 页面 | 建一个待办跑完 |
| 搜索面板 | `Search*` 页面 | 搜一次并跳转 |
| 设置八页、快捷键 | `SETTINGS`、`KEYMAP`、`SHORTCUT`、`theme-scenes` | 改键后重启，确认还在 |
| 主题、透明、背景图 | `theme-scenes` 六个场景 | 肉眼看透明窗口 |
| 完成动画 | `COMPLETE`、`IconMascot` 等页面 | 标记完成一次 |
| 插件和市场 | `plugin_tests`、`market_tests`、`plugins_cmd_tests`、`sidebar_wasm_tests` | 装卸一个插件 |
| `mooring` 命令行 | `managed_session`、`--skill` | Agent 在终端里调一次 |
| 手机连接 | `remote_v2`、`remote_v2_agents`、`vectors`、`mobile` 测试、`PHONES` | 真手机配对、聊天 |
| 多机 | `remote_*`、三个 `remote-*-evidence.sh`（假 ssh） | 真 SSH 连另一台 |
| 中继 | `relay`、`old_relay`、CI 镜像任务 | 连已部署的中继 |
| 通知、菜单栏 | `desktop.rs` 测试、`Menubar` 页面 | 收到一次系统通知 |
| 诊断页 | `DIAGNOSTIC`、`perf-gate` | 打开看一次 |
| Keepline 老安装兼容 | `legacy.rs` 测试 | – |
| 打包和启动 | `package-macos.py` | 干净环境启动 App（`env -i open`） |

## 基线结果

`eedc95a`（#2–#7、#1 合入后），固定路径的独立 worktree，全程 HEAD 没变。测量结果在本机 `~/Library/Caches/MooringVisual/split-baseline-eedc95a/{a,b}`，同一台机器验收可以直接复用；换了提交（比如 G1 合入）就要重测。

| 检查 | 结果 |
|---|---|
| `cargo fmt --check`、默认 clippy、开 `visual-test` 的 `cargo check` | 通过 |
| `sh ci-test.sh` | 793 通过，0 失败，26 忽略，无重跑 |
| `cargo test -p mooring-app --features visual-test` | 215 通过，0 失败，3 忽略 |
| `mobile`：`bun test`、`bun run typecheck` | 61 通过，0 失败；类型检查通过 |
| `eval/protocol-contract/check.py`、`theme-scenes.py` | 通过 |
| `perf-gate.py` | 一遍失败一遍通过：`settings-open` 超预算（+26.3%），本身波动大，其余各项稳定。不是拆分造成的 |
| 场景和页面组 | 44 组里两遍都过的 24 组；其余见上文，受机器负载影响 |

测试名单在 `baseline-tests.txt`，共 1037 条（默认加开 `visual-test` 的 app 测试）。同名测试保留多条，对比时按条数算。

**已知不稳定的用例**：`events_stream_every_change_once_in_order_and_a_stalled_reader_blocks_nobody` 单独连跑 6 遍失败 2 遍，报错位置还不同（`managed_session.rs:4658`、`:4527`）。它失败不算拆分回归，除非 PR #15、#16 修好之后还失败。

**基线还没跑**：中继镜像（CI 在跑）、被忽略的用例、真机走查。
