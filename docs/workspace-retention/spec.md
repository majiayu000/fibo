# 工作区回收：结束的终端和会话什么时候离开工作区

状态（2026-11-05）：方案，待用户拍板，未改代码。对应审计清单 C2，补 [plan.md](../plan.md) 第 3 阶段「结束的终端回收（Reclaimed）」没覆盖的部分：那一条只写了释放 fd 和缓冲（`host/live.rs` 的 `release_ended` 已经在做），没写库里的行和内存里的 `Workspace`。

## 要支撑的决定

- 结束的终端、它上面跑过的会话，在满足什么条件后离开宿主内存里的 `Workspace`（从而不再被每次 commit、订阅、Workspace 回复克隆和扫描），库里的行是留着当归档还是删掉。
- 指标沿用 plan.md 第 3 阶段的验收，并补上行数：开关 200 个终端后，fd、内存、`terminals` 行数、内存里的 `agent_sessions` 条数都回到基线。

## 现状（11-05 摸底）

### 代码

- **没有删除路径**：`Store::save_applying` 对 `terminals`、`agent_sessions` 只有 upsert，没有 `removed(...)` 分支；标签有（关标签删 `tabs` / `tab_layout_nodes` / `tab_order`）。宿主里也没有任何地方从 `workspace.terminals` 或 `workspace.agent_sessions` 里移除元素。`outcome_history`、`lost_agents` 同样只增不减；`requests` 已经按 24 小时清理。
- **启动时全量装入**：`Store::load` 读出全部 `terminals`、`agent_sessions`，`runtime.rs` 再给每个会话从日志索引取一份 `Content` 放进 `workspace.content`。
- **`Host::snapshot()`（`host/mod.rs`）**：先 `self.workspace.clone()` 整份克隆，再保留 `s.terminal.is_some() || index.recent(s.updated)` 的会话（`recent_days` 默认 7 天）和它们的 `content`。所以：
  - 在 Mooring 里开过终端的会话，`terminal` 一直是 `Some`，永远留在回复里；
  - 其余会话 7 天后不出现在回复里，但一直在宿主内存里，每次都被克隆一遍再丢掉。
- **每改一次版本号的固定开销**（Agent 输出时约每秒一次）：
  - 调用方 `let mut next = host.workspace.clone()`（宿主里 28 处）——全量克隆 1 次；
  - `save_applying` 用 `changed()` 给 terminals、tabs、agent_sessions 各建一张 HashMap 再逐条比较——全量扫描 1 次；
  - 有 hook、`fibo events` 订阅者或 `fibo run` 汇报时，`snapshot()` 再全量克隆 1 次，`events::Tracker` 扫一遍；
  - 窗口看到 `workspace_revision` 变大就发 `Request::Workspace`（`workbench/mod.rs`），菜单栏也会发（`desktop.rs`），手机列表读同一份回复（`remote/list.rs`）：每次又是 `snapshot()` 的全量克隆，加上把保留下来的部分序列化成 JSON。
  - 合计：每个版本号至少 3 次全量克隆、1 次全量比较、1–2 次回复序列化。

### 本机数字（`~/.local/share/mooring-workbench/workbench.sqlite`，`sqlite3 -readonly`，11-05 16:19）

没有 `~/.local/share/keepline-workbench`。库建于 11-03（`machine-id`），`requests` 最早一条 11-04 02:54，所以这是一个只用了约两天的库。

| 表 | 行数 | 说明 |
|---|---|---|
| `terminals` | 3 | 全部 `Running`，全部在标签里 |
| `tabs` | 2 | |
| `agent_sessions` | 608 | 绑着终端的 2 条（当前、Open），其余 606 条来自日志索引，从没在 Mooring 终端里跑过 |
| `meta.revision` | 11090 | 约 44 小时，平均约 250 次/小时 |

`agent_sessions` 按 `updated` 的年龄：

| 年龄 | 没绑终端 | 绑着终端 |
|---|---|---|
| < 1 天 | 210 | 0 |
| 1–7 天 | 256 | 2 |
| 7–30 天 | 140 | 0 |

- 结局（`outcome`）全部是 `Open`；`outcome_history`、`lost_agents`、`pending_tasks`、`tasks` 都是 0 行。
- 会话最早一条 10-27，最新 11-05：10 天约 600 条，**约 60 条/天**。
- 这 608 条会话对应的 `session_content.projection` 共 1544 KB（平均 2.6 KB/条），其中 `snapshot()` 留下的 468 条共 1263 KB。`agent_sessions` 行本身约 160 KB 字符串。内存里的 `Content` 是展开的 String / Vec，克隆一份是几千次分配，实际字节比 JSON 大。

### 结论

- **本机现在的大头不是终端行**（只有 3 行），而是日志索引带进来、再也不走的会话：每次克隆 608 条、约 1.5 MB 内容，其中 140 条（23%）克隆完就丢。
- 按 60 条/天推，一年后内存里约 2.2 万条会话、约 57 MB 内容，每个版本号克隆 3 次以上；回复因为 7 天窗口不会跟着涨，但克隆、比较和事件扫描都随总数线性变慢。
- 终端这条路径在本机还看不出来，但很确定：每次 `Launch` 加一行 `terminals`，上面跑过的会话 `terminal` 变成 `Some`，从此永远留在回复里。plan 的「开关 200 个终端」验收在现有代码上必然不过：200 行终端、最多 200 条会话不会回到基线。
- 没有测进程内存和单次克隆耗时（没起基准）；上面的「一年」是线性外推。

## 一个行能不能走，牵涉到什么

| 依赖 | 现在怎么读 | 行走了会怎样 |
|---|---|---|
| `status::of` → `live_terminal` | 要 `terminals` 里有这行且 `Running` | 结束的终端本来就不是 live，不受影响 |
| `recorded()` 的 `left_its_terminal` | PR #18 后只看 `s.terminal.is_some()`，不看终端行 | 删终端行不影响；**但不能把 `s.terminal` 清成 `None`**，否则日志停在提问上的会话会重新变成「等你回答」 |
| `recorded()` 的 `failed` | 找终端行，`Failed` / `Interrupted` / 退出码非 0 就是 Error | 会话还在、终端行先删了，Error 会变成 Review 或 Stale。所以终端行只能跟它的会话一起走，或在会话之后走 |
| `pending_tasks`、`lost_agents`、`tab_layout_nodes` | 外键指向 `terminals(id)` | 要先删这些行；`lost_agents` 是「重开中断的终端」的依据，标签还在时不能删 |
| `agent_launch`、`session_names`、`agent_bindings`、`outcome_history` | 按会话 id | 会话走了，这些一起走或一起归档；`review_since` 读 `outcome_history` |
| 待办 | 会话的 `task_id`；待办详情列出它的会话作证据 | 挂在未关闭待办上的会话不能走 |
| 侧栏「今天 / 更早」、关闭的会话（`sidebar_wasm::Closed` 的 `total`） | 全部来自 Workspace 回复 | 走了就看不到，也不计数 |
| 手机列表（`remote/list.rs`）、`remote/resume.rs` | 同一份 Workspace 回复 | 同上 |
| 日志索引再看到这个对话（在外面 resume 了） | 按 `(runtime, cli_session_id)` 找会话，没有就新建 | 表上有 `UNIQUE(runtime,cli_session_id)`：内存里没有、库里有，新建会撞约束；库里也删了，新建出来是 `Open`，你给的结局、名字、待办都丢了 |

## 可选方案

### A 按时间删库

结束超过 N 天（例如 30 天）的终端和会话，从内存和库里一起删，连带外键行。

- 改动：`save_applying` 加 `removed()` 分支；宿主定时（或每次 commit 顺带）挑出过期的。约 1 个文件加一个清理函数。
- 代价和风险：
  - 你标过的结局、改过的名字、和待办的关联都没了；日志还在的对话之后被 resume，会以 `Open` 重新出现，可能又进「要你处理」。
  - 绑过终端的会话要等 N 天才走，开关 200 个终端的验收要靠测试里拨时钟。
  - N 是一个和 `recent_days`（7）、`stale_days`（本机 3）都不一样的第三个数。

### B 按「可达性」移出内存，库里留作归档（推荐）

宿主内存里的 `Workspace` 只放工作集；不在工作集里的会话留在库里，不装入、不克隆、不发出去。终端行在没人引用后删掉。

一个会话留在工作集，只要满足任一条：

1. 它的终端还在某个标签里，或还在跑，或在 `closed` 里等收尾；
2. `status::needs_you` 为真（要你处理）；
3. 挂在一个没关闭的待办上；
4. 暂存（`Deferred`）还没到期；
5. `session_updated` 在窗口 W 以内（W 待定，默认等于 `recent_days` 7 天，和现在外部会话的规则一样）。

都不满足就移出内存：从 `workspace.agent_sessions`、`workspace.content` 里拿掉，库里的 `agent_sessions` 及其 `agent_launch`、`session_names`、`agent_bindings`、`outcome_history` 行都留着。启动时 `load` 用同一个规则只装工作集。

终端行：已结束、不在任何标签里、没有留在工作集里的会话指着它时，删掉，连带 `pending_tasks`、`lost_agents`（`lost_agents` 只在标签还在时有意义）。

日志索引再看到一个已移出的对话时，先按 `(runtime, cli_session_id)` 从库里把原行读回来，带着原来的结局、名字、待办回到工作集，而不是新建。

- 改动：core 加一个判定函数（放在 `status` 旁边，和 `needs_you` 同一个主人）；store 加「按规则装入」「按 id 读回一条」「删终端行」；宿主在 commit 后、启动时各调用一次；索引新建会话的入口先查库。预计 core 1 个新文件、store 1 个新文件、宿主 2–3 处调用。
- `snapshot()` 的 `terminal.is_some() || recent(...)` 过滤由这个规则替代，内存里有什么就发什么。
- 代价和风险：
  - 「最近关闭」「更早」只显示 W 以内的；更早的要另做一个「归档」视图才能看到（不在本方案内）。
  - 读回路径是新的：漏掉一个新建入口，就会撞 `UNIQUE` 约束或建出重复会话。验收里专门测。
  - 归档行仍在库里：按 60 条/天、每条约 330 字节（行加名字等），一年约 7 MB，可以接受；需要时再加 A 的按时间删库。
  - 窗口、手机看到的集合从「绑过终端的永远在」变成「W 以内或还有事」：绑过终端、结束超过 W、没结局的会话会从侧栏消失。这是要你拍板的语义变化。

### C 不减数量，改成共享不可变数据

`Workspace` 里的会话和内容换成 `Arc<...>`（或 `im` 这类持久化结构），克隆只复制指针；`changed()` 先比指针。

- 好处：不改任何语义，所有历史照旧可见。
- 代价和风险：
  - `Workspace` 是 core、宿主、窗口、手机共用的类型，序列化、守卫测试（`tests/guard.rs` 的原始字段计数）、28 处 `workspace.clone()` 后的就地修改都要改，改动面最大。
  - 只解决克隆，不解决数量：比较、事件扫描、启动装入、`status::of` 遍历仍随总数线性增长；回复里绑过终端的会话照样永远在；开关 200 个终端后内存条数不会回到基线，plan 的验收依旧不过。

### 比较

| | 内存条数回到基线 | 每次克隆 | 保留结局 / 名字 | 语义变化 | 改动面 |
|---|---|---|---|---|---|
| A 按时间删库 | 是（N 天后） | 随 N 天内总数 | 否 | 有：resume 后变 Open | 小 |
| B 可达性 + 归档 | 是 | 随工作集 | 是 | 有：W 外的从界面消失 | 中 |
| C 共享不可变 | 否 | 指针复制 | 是 | 无 | 大 |

## 推荐

做 B。理由：

- 它是唯一既让开关 200 个终端后回到基线、又不丢你给过的结局和名字的方案。
- 它把「谁该留着」收成一个判定函数，替掉 `snapshot()` 里那条混了终端和时间的过滤，符合 plan.md 原则 7（一个事实一个主人）。
- 本机现在 23% 的会话克隆完就丢，B 直接省掉；C 的收益要到一年量级才明显，且改动最大。

C 不做，除非 B 落地后放弃条件里的测量仍然超标。A 留作 B 之后的可选项：只在归档行真的变大时，按很长的期限（例如 180 天）删库。

### 分步

| 步 | 做什么 | 验收 |
|---|---|---|
| B1 判定函数 | core 新增「留在工作集」判定，`snapshot()` 改用它（只换过滤，内存暂不移出） | 对 `sample::every_state` 和本机导出的工作区，判定为留下的每个会话，`status::of` 与改前一致；回复里的会话集合只在「绑过终端、结束超过 W、无事」这一类上变少 |
| B2 移出内存 + 读回 | commit 后、启动时按规则移出；索引新建前先从库读回 | 移出后再写一条它的日志：会话回来，结局、名字、待办都在，没有 `UNIQUE` 错误，没有重复 |
| B3 删终端行 | 已结束、不在标签、无人引用的终端行连同外键行删除 | 开关 200 个终端（含跑过 Agent 会话的）、拨过 W 后，`terminals` 行数、内存里的 `agent_sessions` 条数、fd 都等于开之前；进程内存在基线 +5 MB 以内 |

## 验收标准（总）

- 开关 200 个终端，其中一部分跑过 Agent 会话、一部分以失败退出：在测试里把 W 拨过去后，`terminals` 行数、内存里 `agent_sessions` / `content` 条数、fd 回到开之前；进程常驻内存在基线 +5 MB 以内。
- 下面这些会话无论多旧都不被移出：在标签里的、还在跑的、要你处理的、挂在未关闭待办上的、暂存未到期的。每类一个用例。
- 以失败退出的终端，它的会话在 W 以内一直是 Error（终端行不会先于会话被删）。
- 被移出的会话因为日志更新回来时，结局、名字、待办关联、`agent_launch` 都和移出前一样。
- 用一个预置 2 万条归档会话的库启动：启动耗时、单次 commit p99 和空库相比相差在 1 ms 以内（工作集一样大）。
- 撤掉 B3 的删除，或把判定函数改回 `terminal.is_some() || recent`，200 个终端的验收用例失败（`Guarded-by`）。

## 放弃条件

- 先测再做：B1 之前，用按本机分布生成的 2.2 万条会话（约一年）测单次 `workspace.clone()` 和一次 commit 的 p99。如果克隆 < 1 ms、commit p99 < 2 ms、进程内存 < 50 MB，就不做内存移出（B2），只做 B3 删终端行加 B1 换过滤，把这份方案改成「不做」。
- B2 的读回要改的新建入口超过 3 处，或守卫测试要放宽才能过：说明会话的来源不止一个，先停下来把「新建会话」收成一个入口，再继续。
- B 落地后在本机跑一周，单次 commit p99 仍比移出前高、或仍随天数上涨：问题不在数量，转去评估 C。

## 不做

- 「归档」视图（查看 W 以外的会话）：等你决定需不需要再说。
- `outcome_history` 的单独清理：它按会话归档，跟着 B 走；本机 0 行，不急。
- 日志索引自己的表（`session_content` 3.7 万行、`log_files` 3.7 万行，库 112 MB 的大头）：它们是索引的缓存，不进 `Workspace`、不被克隆，不属于 C2。另开一项再看。

## 要你拍板的语义问题

1. **W 是多久？** 结束了、没结局、也没事的会话，在侧栏「更早」、鸟瞰和手机列表里还要看到多久：
   - 7 天（和现在外部会话一样，推荐）；
   - 30 天；
   - 一直在（等于不做 B，只能走 C）。
2. **在 Mooring 终端里跑过的会话要不要特殊对待？** 现在它们永远在；按 B，结束超过 W 后和外部会话一样离开。还是要比外部会话留得更久（例如 W 的 2 倍）？
3. **你明确标了「完成」或「放弃」的会话，是不是可以更早离开**（例如 24 小时），而不是和没结局的一样等 W？
4. **库里的归档行要不要删？** 一直留（一年约 7 MB，推荐），还是超过某个期限（例如 180 天）删掉，代价是之后 resume 回来变成 `Open`？
5. **手机列表和桌面用同一个 W 吗？** 推荐相同；手机屏幕小，也可以更短。
6. **挂在已关闭待办上的会话**：随待办一起留着当证据，还是按 W 走？推荐按 W 走，待办详情里显示「N 个会话已归档」。
