# 会话开在独立的 worktree 里

状态：方案，待用户拍板，未改代码。对应 [#600](https://github.com/majiayu000/fibo/issues/600)（11-26 竞品对照，汇总在 #621）。

## 要支撑的决定

- 同一个仓库并行跑几个 Agent 时，每个会话能不能一步开在自己的 git worktree 和分支里：从哪些入口开、谁来建、建在哪、收尾时怎么回收。
- 指标：同一空间并行起两个 Agent，各在自己的分支和目录，改同一个文件互不覆盖；会话收尾后 worktree 能被回收，不留半建的目录或分支。

## 现状（2026-11-28，main 3075da01）

### 三个入口

| 入口 | 怎么起会话 | 目录从哪来 |
|---|---|---|
| 窗口：新标签、分屏、待办「开始」 | `workbench/mod.rs` 的 `launch` → `launch_slot.rs` 的 `launch_in_slot`：先起一个 shell（`launch_argv`），再把 Agent 的启动命令（`start_command`，Agent 来自 `tasks.chosen_agent`）打进这个 shell | 恢复的会话 → 待办所在空间的目录 → 插件命令的目录 → 被替换的终端 → 当前标签里继承的终端 → 空间的起始目录（`space_start`），最后过 `usable_cwd` |
| `fibo run <agent> <任务>` | `control/run_agent.rs`：在调用方终端旁边开（`open`），输入就绪后交任务，每轮结果回报给调用方 | 调用方终端此刻的目录 |
| 手机「新任务」 | `mobile/src/app/new.tsx` → host 的 `open`（`remote.rs` 里的 `writes::open`） | 选的目录，没选就是空间目录 |

- host 收到 `Request::Launch { space, argv, cwd, start, .. }`；目录不在了，按 `host/start_dir.rs` 退回空间目录或家目录。
- 空间归属（[2026-11-27 决定](../decisions/2026-11-27-session-space-is-its-tab.md)）：会话算在它终端所在标签的空间。worktree 放在空间目录外面也不影响归属：从哪个空间开的标签，就算哪个空间。
- 现在没有任何入口会新建 worktree。已有的 worktree 只在两处出现：首次运行选目录时认得出（`workbench/first_run_folder.rs`）；诊断里列出「已合入、没人在用」的可回收 worktree（`host/client/worktrees.rs`，#561），`in_use` 会跳过还有终端或进程在里面的。

### Agent 自带的参数（本机 `--help`，11-28）

- Claude Code 2.1.300：`-w, --worktree [name]`（Create a new git worktree for this session），另有 `--tmux` 依赖它。
- Codex 0.166.0：`--worktree`（Run the session in a new managed Git worktree）。
- 其他 Agent 本机没装，未查。两者都没在 `--help` 里写 worktree 建在哪，由各自 CLI 管。

## 可选方案

### A Fibo 自己建 worktree（推荐）

- 「在新 worktree 里」是三个入口共用的一个选项。host 新增一个请求（例如 `Request::Worktree { space, from, name }`）：在 `from`（空间目录，或调用方终端的目录）所在的仓库里跑 `git worktree add -b <分支> <路径> <基点>`，回复路径和分支；之后照常 `Launch`，`cwd` 就是新路径。
- 分支名默认取待办标题或任务第一句，可改；冲突时加序号，不覆盖已有分支。
- 好处：12 种 Agent 都能用；启动前 Fibo 就知道终端目录是哪个 worktree，侧栏分支、看改动、打开文件都指对地方；回收直接复用 #561 的诊断。
- 代价：host 多一处 git 写操作，按 [11-16 决定](../decisions/2026-11-16-audit-safety-boundaries.md) 走 core 的 `git.rs` 和它的安全参数；要处理建失败（不是 git 仓库、分支已存在、目录已存在、子模块、钩子失败），失败时不能留下半建的目录。

### B 借用 Agent 自带的 `--worktree`

- 只对 Claude、Codex 出现这个选项，在启动命令后面加参数，Fibo 不碰 git。
- 问题：只覆盖 2/12；worktree 建在 CLI 自己管的位置，终端的目录却还是仓库根，Fibo 只能从记录里的 cwd 反推，`terminal_cwd`、侧栏分支、看改动都会先指错地方，要再补一层对应；回收归 CLI，诊断不认识这些目录。

### C 两者都做

- A 为默认，Claude、Codex 另可走自带参数。两条路的显示和收尾都要各写一份，收益只是省一条 git 命令。

### 比较

| | A | B | C |
|---|---|---|---|
| 覆盖的 Agent | 12/12 | 2/12 | 12/12 |
| Fibo 知道终端在哪个目录 | 启动前就知道 | 要从记录反推 | 两种都有 |
| 回收 | 复用 #561 | 各 CLI 自己管 | 两套 |
| 新代码 | host 一个请求 + 三处入口 | 启动命令加参数 | 两者之和 |

## 推荐

A。分步，每步一个 PR：

1. core + host：`git.rs` 的 `worktree_add`、host 的 `Worktree` 请求；失败原样说明（走 `t!`），不留半建目录。单测：临时仓库里建成；分支已存在时报错且不覆盖；非 git 目录报错。
2. 窗口：新标签和待办「开始」选 Agent 的地方加「在新 worktree 里」开关（默认关，记住上次的选择），开着时先 `Worktree` 再 `Launch`。侧栏会话行在分支和空间默认分支不同时显示分支名。
3. `fibo run --worktree [name]`：同一个请求，`from` 是调用方终端的目录；`--json` 带上 `worktree` 路径。
4. 手机新任务：`open` 请求加 `worktree`，界面加同一个开关；`docs/phone-app/walkthrough.md` 加一项走查。
5. 收尾：会话标完成后，诊断的可回收清单里标出「Fibo 为会话建的」。不自动删：删掉会丢没合并的提交。

## 接口和改动的文件（预估）

- `crates/fibo-core/src/protocol/request.rs`：`Request::Worktree`；回复带路径和分支。旧 host 不认得时回 `Reply::Error`，窗口提示「后台版本太旧」（CLAUDE.md：换了版本，旧的东西还在）。
- `crates/fibo-core/src/git.rs`：`worktree_add`。
- `crates/fibo-host/src/host/client/`：新文件处理 `Worktree`。
- 窗口：`workbench/launch_slot.rs`；选 Agent 的三处（`tasks.rs`、`render/bars.rs`、`layout/menu.rs` 设 `chosen_agent` 的地方）；侧栏会话行。
- `crates/fibo-host/src/control/run_agent.rs`、`control/open.rs`。
- `crates/fibo-host/src/remote/` 的 `open`、`mobile/src/app/new.tsx`。
- 定下来后写一条 `docs/decisions/`（跨三个入口，属于活得比一次改动久的决定）。

## 验收标准

- 同一空间并行起两个 Agent（窗口一个、`fibo run` 一个），各在自己的 worktree 和分支，改同一个文件互不覆盖。
- 侧栏行看得出各自的分支；这两个终端的 `terminal_cwd` 是各自的 worktree；看改动只列本 worktree 的改动。
- 非 git 目录、旧 host、分支已存在：开关不可用或给出清楚的错误，磁盘上不留半建的目录和分支。
- 会话标完成后，它的 worktree 出现在诊断的可回收清单里，标「Fibo 为会话建的」。
- 换包验证：旧 host 不认得新请求时，窗口给出提示而不是卡住。

## 放弃条件

- host 建 worktree 的失败情况（子模块、LFS、sparse checkout、仓库钩子）在真实仓库里多到处理不完时，退回 B，只给 Claude、Codex。

## 不做

- 自动合并、自动删除 worktree。
- 给每个 worktree 分端口、跑 setup 脚本（Conductor 有）：等 A 用起来后看需要，另开。

## 要你拍板的问题

1. worktree 放哪：仓库旁边的 `<仓库名>-worktrees/<名字>`（推荐，和现在 `~/code/fibo-worktrees` 的用法一致，不进仓库）、仓库里的 `.fibo/worktrees/`（要进 `.gitignore`），还是 runtime 目录下？
2. 分支名要不要统一前缀（例如 `fibo/`）？
3. 新分支从当前 HEAD 开（推荐，和你此刻看到的代码一致），还是从默认分支开？
4. 开关默认关（推荐），还是对 git 仓库默认开？
