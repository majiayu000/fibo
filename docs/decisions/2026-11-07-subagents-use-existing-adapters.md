---
status: accepted
date: 2026-11-07
decision-makers: 用户、Codex
---

# 子 Agent 沿现有 Adapter 接入，共用父子索引和侧栏列表

## Context

用户要求把 Codex、OpenCode、Cursor 的父子会话/子 Agent 都接入，询问是否应采用策略模式以及现有架构是否混乱。

`mooring-host::agents::Adapter` 已经是按 CLI 选择的策略接口。Claude 的 `subagents()` 读取独立元数据；Codex 的 `links_children()` 使用子日志中的父线程 ID。两种数据都由 core 的 `Content::keep_subagents` 筛选并输出 `Subagent`，侧栏无需识别 CLI。

缺口在输入：OpenCode 声明只选 `parent_id IS NULL` 的根会话，Cursor 扫描跳过 `subagents/`；共享子日志查询又没有隔离 CLI 名称空间。OpenCode 原查询只返回毫秒游标 `_t`，没有解析器需要的秒级 `timestamp`。

## Decision

采用现有 Adapter；不增加第二套 Strategy/Factory、按品牌分支的 UI 或新的状态表。

- Codex 从 session metadata 读取父线程 ID，保持现有方式。
- Cursor 从 `<parent>/subagents/<child>.jsonl` 读取父子 ID，工作目录沿父会话元数据取得。子任务名取其第一条任务文本。日志的 `turn_ended` 是明确的完成/中断/失败信号。
- OpenCode 保持声明式插件；`transcripts.parent` 指向源记录中的父 ID。SQLite mirror 透传查询选出的会话元数据，并在消息前写入，避免子会话先以顶层会话出现。查询自己把毫秒转换为 `timestamp` 秒数。
- 父子查询按 `(runtime, session_id)` 区分；子线程的新日志沿共享 index 路径更新父会话。
- Claude 保留官方元数据读取策略，不强制把它的 meta 文件伪装成子会话日志。
- 四种工具共用 `Content.subagents`、回合筛选和现有侧栏折叠列表。当前列表展示直接子任务，不增加递归树或子线程终端控制。

## Consequences

新增来源应在自己的适配器或声明里归一化事实，不能要求 Host/侧栏识别 OpenCode、Cursor 等品牌。两种来源是存储方式差异，不是两套 UI 语义。

新增的父字段使用现有插件级别门槛 6；OpenCode 插件为 0.2.2，旧 App 在安装时会要求更新。目录签名和 index 仍由发布工作流生成。

Cursor 没有给每条记录写时间；用户消息时间沿用现有 timestamp 解析，其他记录沿用文件时间，不能当作精确事件遥测。关系按磁盘中明确记录的归属呈现，不根据任务文本、目录相似性或并行启动时间推断。

## Implementation Plan

- `crates/mooring-core/src/agent_transcripts.rs`：从达到文件大小上限的 agent.rs 拆出已有声明类型，并补父字段。
- `crates/mooring-host/src/agents/{cursor,declared,mirror}.rs`：各来源的归一化。
- `crates/mooring-host/src/content/children.rs`：从 db.rs 拆出已有父子投影，按 CLI 隔离。
- `crates/mooring-host/src/log_index*`：复用日志监听、持久化、父会话更新。
- `plugins/opencode/agent.json`：读取父子关系、时间转换、问题回答后的继续事件。

## Verification

- `cargo test -p mooring-host --test subagent_adapters`：OpenCode SQLite → mirror → adapter → index；Cursor 子日志 → 同一个 index；同 ID 跨 CLI 隔离、运行/等待/结束/中断、父日志不动、重启。
- `cargo test -p mooring-host --lib`：Codex、Claude、mirror 与 Cursor 相邻路径。
- `cargo test -p mooring-host --test architecture_boundary`：适配器之外不识别 CLI 品牌。
- `cargo test -p mooring-core --test guard --test file_size`：单一事实所有者、文件大小约束。
- 原生窗口同源数据前后对照；撤销父关系接入后的回归失败。

真实日志回放使用 `MOORING_REPLAY_OPENCODE_DB` / `MOORING_REPLAY_CURSOR_CHILD`，单独运行 `cargo test -p mooring-host --test subagent_adapters replay_local_child_records -- --ignored --nocapture`（Agent 注册表是进程内单例）。源数据库只读；Cursor 原文复制进临时目录后回放。旧样本越过日期窗口只发生在测试，生产查询不回填历史。

实际命令结果记录在提交说明和本地验收产物中。

## Alternatives

- 新建 SubagentStrategy 工厂：现有 Adapter 已表达来源能力，额外工厂只会增加路由和接口。
- 每种 CLI 单独写侧栏：重复状态、计数、折叠逻辑，后续必然出现差异。
- 只移除 OpenCode 顶层过滤：子会话会混进普通会话列表，未完成归属。
- 用目录或任务描述猜关系：真实日志已经提供明确父 ID 或目录归属，不采用猜测。

## More Information

仅在新的真实来源无法通过现有元数据/子日志两种入口表达时，重新评估接口。此决定不授权历史回填、兼容层或额外配置模式。
