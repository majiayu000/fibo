# 第 1 阶段：编辑不再被挡、不再丢

状态（2026-11-03）：开工。对应 [plan.md](../plan.md) 第 1 阶段（I1–I7、I10、I11），也是多机第 2 步（远程打字和审批）的前提。

## 要支撑的决定

- 窗口、`mooring` 命令、手机和另一台电脑发来的写请求，能不能都走同一套「信封 + 回执 + 去重」，让用户在 Agent 持续输出时做的操作不再被拒、不再被悄悄丢掉，断线重发也不会做两遍。
- 指标就是 plan.md 的验收：
  - Agent 持续输出时连续标记完成 100 次，0 次被拒；
  - 回执前断开、再用同一个 id 重发，待办只多一个；
  - Agent 分屏的同时改标签名、拖分隔线，三个改动都在；
  - 关标签的确认框打开后 Agent 新开了分屏，关标签被拒，要求重新确认。

## 现状（11-03 摸底）

- **只有一个全局版本号**：`Workspace.revision` 只在 `Host::commit` 里加一，但会话输出、绑定、标题和画面提示、后台 shell 也都走 commit，Agent 输出时大约每秒加一次。
- **9 种编辑带全局版本号检查**，对不上就回「工作台已变化，请刷新后重试」。这 9 种是：AddSpace、UpdateSpace、AddTask、UpdateTask、SetTaskOutcome、SetPendingTask、SetOutcome、AssignTask、SaveSessionTask。其余编辑不检查，按 id 直接应用。
- **App 忙时会丢请求**：App 只有一个 `busy` 标志，忙时只排队「不查版本号」的编辑，其他请求直接丢掉，界面上不说。
- **布局靠整块覆盖**：拖分隔线、移动窗格、改标签名、开会话、启动后放窗格，都发 `PutTab` 整块覆盖。
- **没有请求 id、没有去重**：重发 AddTask 会回「ID 已存在」的错。

## 分步

| 步 | 做什么 | 验收 |
|---|---|---|
| A 按对象带前提 | 去掉全局版本号检查。SetOutcome 带「我看到的结局」和 turn_key，SetTaskOutcome 带「我看到的结局」；AddTask、AddSpace、AssignTask、SetPendingTask、SaveSessionTask 不带前提；UpdateSpace、UpdateTask 按 id 改 | Agent 持续输出时连续标记完成 100 次，0 次被拒；Agent 在你看过之后又开了新一轮，「标记完成」被拒并说明原因 |
| B 信封、回执、去重 | 写请求外面包一层 `{client, request, edit}`；回执分三种：已落库、被拒（附对象现状）、重复（返回上次结果）；`requests` 表和改动在同一个事务里写，保留 24 小时 | 回执前断开、用同一个 id 重发，只生效一次；宿主重启后重发也一样 |
| C 一个请求状态机 | App、CLI、手机共用：挂起 / 已确认 / 被拒 / 结果未知。忙时不再丢，排队；表单等回执才关；超时显示「不确定是否成功」，给出用同一个 id 重试的按钮 | 拔掉宿主再接上，挂起的请求要么确认、要么显示结果未知，没有一条无声消失 |
| D 布局改成操作 | CreateTab、RenameTab、MoveTab、SplitPane、ResizeSplit、MovePane、ClosePane{expect_terminal}、CloseTab{expect_terminals}；去掉 PutTab | Agent 分屏的同时改标签名、拖分隔线，三个改动都在；关标签确认后有新分屏，关标签被拒 |
| E 远程协议用信封 | `mooring remote` 的写操作带上 client 和 request，回执原样传回去 | 手机和另一台电脑断线重发，只生效一次 |

- A 先做：改动最小，单独就能解决「被挡」。
- B 要改存库：`Store::save` 接收可选的请求记录，写进同一个事务。

## 放弃条件

- A 做完后，用 Agent 持续输出的真终端测「标记完成」，如果被拒仍然超过 1%，说明问题不在全局版本号，回到设计重新找原因。
- B 的去重表让每次 commit 的耗时 p99 增加超过 5 ms：改成只给用户请求写表，内部 commit 不写。

## 不做

- UpdateTask 拆成按字段改（plan.md 原有）：待办只有窗口一个写入方，宿主、`mooring` 命令和手机都不改待办，所以整块覆盖冲不掉别人的改动。等出现第二个写入方（比如手机能改待办）再拆。

- 会话内容单独计数（plan.md 第 1 条）放到第 3 阶段和写库线程一起做。A 去掉全局检查以后，内容加一不会再挡住编辑；剩下的问题是每秒一次整表对比的写库开销，那属于第 3 阶段。

## 结果

### A 按对象带前提（2026-11-03）：完成，待真机验收

- 宿主不再核对全局版本号。`Edit::SetOutcome` 带 `seen: { outcome, turn }`，`SetTaskOutcome` 带 `seen`，都由 core 的 `Workspace::stamp` 按窗口当时的副本填写，窗口的 `edit()` 统一调用它；宿主只在这个会话的结局变了、或 Agent 开始了新一轮时拒绝，并说明原因。已经是目标结局的，直接算成功。脚本不带 `seen`，照旧应用。
- App 忙时，除新建（AddTask、AddSpace、SaveSessionTask）以外的编辑都排队，不再丢；新建仍丢，防止双击建出两个，等 B 有请求 id 后再改。
- `Request::Edit` 的 `revision` 字段暂时保留但不再读，B 用信封替换它。
- 已验：core 单测（Agent 输出把版本号推高 100 之后，在旧副本上做的「标记完成」照样生效；重发不报错；新一轮、被别人改过、待办结局被改过都被拒）；宿主集成测试（版本号过期的编辑只因对象不存在而失败）；core、宿主、App 全部测试，守卫测试。
- 验收测试 `marking_done_while_the_workspace_moves_is_never_refused`：真宿主进程绑上一个 Codex 会话，另一个线程每 2 ms 提交一次让版本号前进（测试中前进几百次），同时用 5 ms 前的副本连续标记完成和重新打开 100 次：0 次被拒。临时放回旧的全局检查后，同一测试 100 次全被拒。
- 11-03 已换进正在用的 App，换包前后 43 个 Agent 进程一致。
- 没验：真 Agent 输出时在窗口里手点「标记完成」；版本号是用宿主请求推的，不是真 Agent 日志。
