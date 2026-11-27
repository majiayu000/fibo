# 换包交接：终端冻结的时间和库大小脱钩，ACK 之前不写库

状态：方案，待用户拍板，未改代码。对应 [#604](https://github.com/majiayu000/fibo/issues/604)（11-26 core/host 架构审查，汇总在 #621）。

## 要支撑的决定

- 交接时旧 host 冻结全部终端（不读输出、不收输入、不提交）的时长，要不要和库的大小脱钩。
- 新 host 在回 ACK 之前能不能写库。
- 指标：冻结时长有数字，进 perf-gate；ACK 之前任何一步失败，旧 host 照常服务，库里也没有新 host 写过的东西（[plan.md](../plan.md) 原则 5「READY 之前失败，旧宿主继续服务」，第 3 阶段验收「在 Prepare / READY / COMMIT / SERVING 前后各注入一次失败，终端都不断」）。

## 现状（2026-11-28，main fc4c954b）

### 旧 host（`host/client.rs` 的 `hand_over`）

1. 拿全局锁，先补登记写库失败的退出（`retry_terminal_exits`，#657），再逐个终端拿 `Live` 锁、暂停空闲终端的输入；有终端还在写，就放开所有锁，等它写完（10 秒内）再重来。
2. `hand_over_paused`：发 `Reply::Handoff`（每个终端的 id、pid、尺寸、标题、cell），逐个终端发快照分块加 `READY`，再用 `term::send_masters` 传 PTY 描述符。
3. 以 10 秒读超时等 ACK，期间锁一直持着（代码注释：「no output is fed, no state is committed」）。收到 `taken` 就 `exit(0)`；否则报 “handoff not confirmed, still serving” 并放锁。

### 新 host（`host/runtime.rs` 的 `serve_runtime` 和 `take_over`）

1. `take_over`：连旧 socket，发 `Request::Handoff`，收快照和描述符，逐个 `Vt::restore`，起读端和输入。
2. 回 ACK 之前还要做：
   - `announce_host`、shell integration；
   - `Store::open_keeping`：读全部行（含已归档的），把没交接的在跑终端改成 Interrupted、写 `lost_agents`、清 `current`，然后 `save`；
   - `Index::open`，对库里全部会话 `index.card()`；
   - `retention::archive`、两次 `index.fork`；
   - 退出监视、绑 pending socket。
3. 写 ACK，再等旧 host 放开 `host.lock`（10 秒）。

### 测试与数据

- 已有 `managed_session/lifecycle.rs` 的 `a_takeover_that_cannot_start_leaves_the_old_host_serving` 和 `a_store_rejected_during_takeover_keeps_the_old_host_and_pty`：ACK 前失败时旧 host 继续服务。两者都没检查库有没有被新 host 改过。
- 没有任何计时，不知道现在冻结多久。库按 [11-24 决定](../decisions/2026-11-24-workspace-retention.md) 只增不删；11-19 记录过一台长期安装有 831 个终端、5000 多条会话。`scripts/perf-fixture.py` 能用 `OLD=K`、`SUB=K` 造大库。

### 两个问题

1. 冻结时长等于传输时间加上新 host 的全部准备，后者随库变大；超过 10 秒，交接必然失败：终端不会断，但新包换不上。
2. `open_keeping` 在 ACK 之前写库。之后任何一步失败，旧 host 照自己的内存继续服务，库却已被改过。例如冻结前刚退出的终端没被交接，新 host 把它写成「后台已重启」并加进 `lost_agents`；旧 host 之后补写 Exited，只修了 `terminals` 那一行，`lost_agents` 那一行一直留着（#606）。

## 可选方案

### 第一步：先量（推荐最先做）

- 两边打点，写进 host 日志：旧 host 记「拿到锁 → 收到 ACK」；新 host 记「发 Handoff → 收齐描述符 → 读完库 → 读完卡片 → 发 ACK」，单位毫秒。
- perf-gate 加交接场景：`perf-fixture.py` 造 `OLD=5000 SUB=700` 的库，起 host、开几十个终端，跑 `fibo serve --takeover`，取冻结时长。有了基线再定预算。

### ACK 之前不写库（推荐，和下面 A、B 都不冲突）

- 把 `Store::open_keeping` 拆成两步：只读的加载放在 ACK 之前；「把没交接的终端标成中断」和 `save` 放在 ACK 之后。
- ACK 前失败：库完全没被碰过，旧 host 的内存和库一致。ACK 后、save 前新 host 崩溃：下次普通启动时 `open_keeping`（没有交接来的终端）照样会把它们标成中断，结果一致。

### A 准备挪到冻结之前，收到描述符后只补差（推荐，视第一步的数字再做）

- 新 host 在发 `Request::Handoff` 之前做完重活：只读打开库、`Index::open`、读卡片、在内存里归档；记下库的 `meta.revision`。
- 收到描述符后（旧 host 已冻结，不再提交），再读一次 `meta.revision`：变了就重读工作区的行。行比卡片便宜得多；卡片之后由新 host 自己的日志索引补齐。
- 冻结时长约等于传输加补差，不再随会话数增长。

### B 只少读卡片（例如只读近 7 天会话的）

- 改动最小，但冻结时间仍随行数增长，治标。

## 推荐

顺序：第一步（量）→ ACK 之前不写库 → 视数字做 A。后两步各一个 PR。

## 接口和改动的文件（预估）

- `crates/fibo-host/src/host/runtime.rs`（`serve_runtime`、`take_over` 的顺序）、`crates/fibo-host/src/store.rs`（拆 `open_keeping`）、`crates/fibo-host/src/host/client.rs`（旧 host 打点）。
- `scripts/perf-gate.py`、`scripts/perf-fixture.py`（交接场景）。
- 交接协议不动：`Request::Handoff`、`Reply::Handoff`、ACK 都保持原样，新旧版本照常互相交接（plan.md 底线：交接协议要么保持，要么在同一次换包里新旧两边都认）。

## 验收标准

- 交接冻结时长有数字并进 perf-gate；`OLD=5000 SUB=700` 下低于预算（预算看第一步的数据定）。
- 在 Prepare / READY（收齐描述符）/ COMMIT（ACK）/ SERVING 前后各注入一次失败：终端都不断；ACK 之前失败时，库和交接前逐字节一致（扩展上面那两条用例，新增这条断言）。
- 新旧版本互换：旧包换新包、新包换回旧包都能交接。

## 放弃条件

- 第一步量出来大库下冻结也只有几百毫秒：A 不做，只做「ACK 之前不写库」。

## 不做

- 改交接协议（例如增量传快照）。
- 把卡片挪出 `Workspace`。

## 要你拍板的问题

1. 冻结时长的预算定多少，例如 1 秒？
2. 「ACK 之前不写库」独立、风险小，可以先做吗？
3. A 的补差：`revision` 变了就重读全部行（简单，推荐），还是只重读变了的行（更快，但要改 store）？
