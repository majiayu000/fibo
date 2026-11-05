# 工作区只增不减（审计 C2）：方案

状态：待定。只写方案，没有改代码；下面「要你定的」三条定了再动手。

## 现在的样子

- 关掉画面或标签时（`host/client/edit.rs` 的 `Edit::ClosePane` / `CloseTab` / `RemoveSpace`），宿主停掉进程，等它退出、输出读完后由 `host/live.rs` 的 `release_ended` 从 `host.terminals` 拿掉活的 PTY。fd 和缓冲这时已经释放。
- 但 `Workspace.terminals` 里的那一行一直留着（状态变成 `Exited`），`store.rs` 的 `terminals` 表也没有删行的地方。`agent_sessions` 一样：表里没有 `DELETE`，启动时全部读进内存。
- `Host::snapshot()`（`host/mod.rs:109`）先整份克隆工作区，再只留 `s.terminal.is_some() || index.recent(s.updated)` 的会话。会话绑过终端后 `terminal` 不会再变回 `None`，所以绑过终端的会话永远留在快照里，7 天的 `recent_days` 对它们不起作用。
- 每次 `commit` 有 Hook、事件订阅或回报时再调一次 `snapshot()`（首次还有一次 `before`），每个 Workspace 回复也是一次；`store.rs` 的 `changed()` 每次保存都给全部终端、全部会话建一张 HashMap 比对。代价都随开过的终端数线性增长，不会回落。

结果：开关 200 个终端后，fd 回到基线，内存、库大小、每次提交和每个回复的耗时不回去。这正是 plan 第 3 阶段的验收「开关 200 个终端后，fd 和内存回到基线」没达到的那一半。

## 方案

分三步，每步一个 PR，各自有测试：

1. **回收终端行。** 终端满足「进程已结束（`Exited` / `Failed` / `Interrupted`）且不在任何标签的布局里」时，在同一次 `commit` 里从 `Workspace.terminals` 删掉，`save_applying` 跟着删 `terminals`、`pending_tasks`、`lost_agents` 里引用它的行（这两张表对 `terminals(id)` 有外键）。会话的 `terminal_id` 不动：B5（#18）之后，终端行不在时状态已经按「绑过终端」判，不会卡在「等你回答」。
   - 测试：开 200 个终端再关掉，`Workspace.terminals` 长度和库里的行数回到开之前；宿主重启后不会再读出这些行。
2. **快照的保留规则改成看终端是不是还活着。** `snapshot()` 留下「当前在一个还存在的终端里」或 `recent` 的会话，而不是「`terminal` 有值」。第 1 步之后，终端行没了的会话 7 天后自然离开快照。
   - 测试：绑在已回收终端上、8 天前更新的会话不在快照里；绑在运行中终端上、8 天前更新的会话还在。
3. **内存里只放快照会用到的会话。** 启动时只读第 2 步规则会留下的会话（加上待办、`outcome_history` 引用到的），其余留在库里；恢复会话、按会话 id 查找的地方没在内存里找到时去库里查一次。这一步改动面最大，先做 1、2 再看压测数字决定要不要做。

不做的：不换持久化数据结构（im / Arc 共享）来省克隆。第 1、2 步之后工作区大小有上限，克隆代价跟着有上限；先量。

## 要你定的

1. 「不在布局里的已结束终端」立刻回收，还是留一段时间（比如能在侧栏里看到「刚退出」）？方案按立刻回收写。
2. `agent_sessions` 表里的旧会话是否永远保留（只是不进内存），还是超过某个天数（比如 30 天、已验收或放弃的）从库里删？方案按永远保留写，删行另开一项。
3. 第 3 步要不要做，还是第 1、2 步后按压测数字再定？

## 怎么量

- 现有 `pty_stall` 压测之外，加一个开关 200 个终端的用例，前后记录 `Workspace.terminals` / `agent_sessions` 长度、进程 RSS、`snapshot()` 和一次 `commit` 的耗时。
- 验收沿用 plan 第 3 阶段：开关 200 个终端后 fd 和内存回到基线。
