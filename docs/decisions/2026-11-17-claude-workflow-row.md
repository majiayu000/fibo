---
status: accepted
date: 2026-11-17
decision-makers: 用户、Codex
---

# Claude workflow 在现有子 Agent 树里占一行

## Context

用户确认采用 option B：workflow 在跑时看清阶段与完成步数，停后保留停在哪里。loom 的真实 run `wf_413a0192-2fc` 有 10 次 Agent 尝试、6 个 journal key、2 个 result；只递归读取 meta 会重复计数，且这些 meta 没有 finished。

## Decision

采用现有 Adapter → Content.subagents → 侧栏／手机通路。一个 workflow 是一个 Subagent，附带 host 已归并的阶段和步骤；普通子 Agent 不增加包装层。`status::of` 不变。

- journal 按 key 保留最后一次 started；result 的 key 和 agentId 都必须匹配当前尝试。结束 run 不抹去已经完成的步骤。
- 优先使用 Claude 同目录 `workflows/<run>.json` 的结构化 workflowName、phases、taskId、startTime、status 和时间。这些正是父日志 Workflow 调用生成的快照，避免执行或猜测脚本里的 JavaScript meta；没有名称时用 run id。
- TaskStop 的成功回执、完成通知、快照和 Stop hook 的精确 workflow task registry 结束 run。新启动回执清掉同 task id 的旧结束证据。会话状态继续由现有后台 Agent 规则计算。
- meta、journal、workflow 快照更新都映射回父日志，复用索引监听；不每次父日志追加都重扫目录。
- 老 Claude 无 journal 时沿 meta 列步骤；会话不在跑时沿用原有 runs(s, live) 的结束兜底。旧索引按既有 RULES 重建机制重新解析，旧客户端忽略新增字段。
- 手机只显示 host 生成的摘要，不实现阶段展开，不再读一次源文件。

## Consequences

停止的 workflow 即使早于当前回合启动，也保留到结束所在回合；后续回合开始后按现有可见范围移除。journal 格式未知、损坏或只有未知事件时退回一行摘要，禁用展开。目录结构改变但 workflow 快照仍在时也能显示摘要；若 Claude 连快照存放方式一起改变，需重新取样。

## Implementation Plan

读取在 `crates/fibo-host/src/agents/claude_workflows.rs`；生命周期及摘要在 `crates/fibo-core/src/subagents.rs`；侧栏只画分组；手机沿现有 cards 增量协议接收摘要。未增加数据库表、配置开关或 workflow 执行器。

## Verification

- `cargo test -p fibo-host --test claude_workflows`：重试、迟到结果、无 journal、未知格式、父日志不动时刷新、flush 后重启、TaskStop／通知／Stop registry 和恢复。
- 设置 `FIBO_REPLAY_CLAUDE_WORKFLOW=<父日志>` 后运行 `cargo test -p fibo-host --test claude_workflows replay_loom_workflow_read_only -- --ignored --nocapture`：只读回放 loom，输出 Verify、2/6、已停，保留四步中断。
- `MOORING_FIXTURE_CHILD_ADAPTERS=workflows python3 scripts/visual-check.py shots <sha> <持久目录> --pages MainSpace@1420,MainSpace@1225`：同一份脱敏 loom 结构的在跑／已停对照。
- `visual-check.py guard` 撤销 Claude Adapter 的 workflow 接入后，重试回归必须失败。
- 手机 lint／类型检查／测试和真机全量走查分别记录，桌面截图不替代真机验收。

## Alternatives

递归把十个 meta 摊平：重复、读不到 journal 的结束证据。新增一套 workflow 面板或改会话状态机：超出当前要求。解析／执行 meta 脚本：快照已有结构化字段，额外解释器没有必要。

## More Information

决定为采用；跨 Claude 版本的结构稳定性仍待验证。本次源结构来自本机真实 run，提交夹具仅保留标签、阶段、key 与生命周期，不提交私有提示词、结果或路径。新版本目录或 journal 不再符合以上结构时，先退回摘要，再取真实样本决定是否恢复展开。
