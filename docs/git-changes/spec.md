# 看改动加上「未提交的改动」视图

状态：方案，待用户拍板，未改代码。对应 [#601](https://github.com/majiayu000/fibo/issues/601)（11-26 竞品对照，汇总在 #621）。

## 要支撑的决定

- 桌面「看改动」除了现在的「这一轮的净改动」（来自会话记录），要不要再加一个按 git 看的「未提交的改动」视图；加的话，在里面能做什么（看、标看过、撤销一个文件），不做什么。
- 指标：Agent 用命令或脚本改的文件也能在桌面看到；桌面能做到手机改动页已经能做的事。

## 现状（2026-11-28，main 3075da01）

### 桌面

- 面板在 `workbench/layout/changes_panel.rs`（另有 `changes_files.rs`、`changes_rows.rs`、`changes_slots.rs`），盖住终端的 56%。打开时向 host 要文件列表，选中一个文件再要它的内容；内容不进工作区快照。
- 数据由 host 按会话记录合成（`fibo_core::turn_diff`），只接了 Claude、Codex 的记录，其他 Agent 只有行数。面板自己写着「用命令或脚本改的文件不在这里」。
- [2026-11-06 决定](../decisions/2026-11-06-turn-changes-from-the-log.md)：本轮内容来自记录，不用 git（非 git 目录也要能看；不混进别的会话的改动）；只读，「不做批注和发回 Agent，11-05 已否定」。

### 手机

- `mobile/src/app/diff.tsx`：工作区对 HEAD 的改动，含未跟踪文件。每个文件一张卡；在手机上标「看过」（`mobile/src/store/reviewed.ts`）；「要求修改」（一句话发给 Agent）；「让它提交」。
- 数据来自 `crates/fibo-host/src/remote/diff.rs`，在 `fibo remote` 桥进程里跑 git。只给「完全控制」的手机；较低权限拿 `diff_summary`（来自记录，只有文件名和行数）。跑法：关 fsmonitor、不用可选锁、不跑 textconv；单文件 64 KB、总共 512 KB 封顶，超出的文件只列行数并标 `truncated`。

### 约束

- [11-16 决定](../decisions/2026-11-16-audit-safety-boundaries.md)：原生 Git 查看共用 `core.fsmonitor=false`、`safe.bareRepository=explicit` 等参数，Git 入口在 core 的 `git.rs`。
- 多机（[11-14 决定](../decisions/2026-11-14-one-window-many-hosts.md)）：窗口用同一套协议直连别的电脑的 host，所以 git 必须由终端所在那台的 host 来跑。窗口自己读分支（`layout/turn_bar.rs` 的 `git_branch`）只对本机成立，不能照搬。

## 可选方案

### A 面板加第二个视图「未提交的改动」，由 host 跑 git（推荐）

- 新请求 `Request::GitChanges { terminal }`：host 用 `terminal_cwd` 定目录，复用 `remote/diff.rs` 的读取逻辑。把它挪到 host 的公共位置，手机和窗口共用一份（同类逻辑只留一个来源）；`remote/diff.rs` 只留权限判断。
- 面板顶部切换「这一轮 / 未提交」；非 git 目录只有「这一轮」。
- 每个文件：看 diff、标「看过」、「撤销这个文件的改动」（确认后对已跟踪文件 `git restore`，未跟踪文件挪进废纸篓而不是删除）。撤销前把补丁存进 runtime，可以再撤回。
- 不加「要求修改」「让它提交」：桌面的终端就在面板下面，直接打字即可；也守住 11-06「只读、不发回 Agent」的界限（撤销文件不往 Agent 发任何东西）。

### B 只加视图，不给撤销

- 最保守。但竞品的核心价值正在撤销：Codex App 能按文件、按 hunk 暂存或撤销，Nimbalyst 每处编辑可以原地批准或拒绝。

### C 撤销到 hunk

- 对单个 hunk 跑 `git apply -R`。可以做，但先把文件级做稳，放到之后。

## 推荐

A，分两个 PR：

1. 只读视图和「看过」：`GitChanges` 请求、面板视图切换、逐文件 diff。
2. 文件级撤销：确认框、`git restore` / 进废纸篓、撤回。

hunk 级以后再议。

## 接口和改动的文件（预估）

- `crates/fibo-core/src/protocol/request.rs`：`GitChanges { terminal }` → 回复沿用手机现在的 `Diff` 形状（`fibo_remote::protocol::Diff`），或把它挪进 core 共用。
- `crates/fibo-host/src/remote/diff.rs` 的读取部分 → host 的公共模块；`host/client/` 加处理 `GitChanges` 的文件；第二步加处理撤销的文件。
- 窗口：`layout/changes_panel.rs` 加视图切换，列表和行复用 `changes_files.rs`、`changes_rows.rs`。新代码放新文件，守住 800 行。
- 性能：只在打开面板、切到这个视图时请求一次（11-06 的「点开时才读，不进工作区快照」）；perf-gate 的 frames-changes 场景覆盖新视图。

## 验收标准

- Agent 用 `sed` 或生成脚本改的文件出现在「未提交」视图；非 git 目录不出现这个视图。
- 撤销一个文件后 `git status` 不再列它；撤回后恢复；未跟踪文件进了废纸篓。
- 另一台电脑上的终端：视图内容由那台的 host 给出（多机验收）。
- 较低权限的手机不受影响，仍然只拿摘要。
- perf-gate 的 frames-changes 不退化。

## 不做

- 行内批注发回 Agent（11-05 否定，记录在 11-06 决定）。
- 一键提交。

## 要你拍板的问题

1. 撤销放不放进第一个 PR？推荐第二步再做，先只读。
2. 「看过」存在 host（桌面和手机共用），还是各端各存（手机现在存在手机上）？
3. 对比的基线：HEAD（和手机一致，推荐），还是分支起点（merge-base，配合 #600 的 worktree 会话更有用，可以作为第三步）？
