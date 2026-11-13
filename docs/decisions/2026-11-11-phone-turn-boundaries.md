---
status: accepted
date: 2026-11-11
decision-makers: 用户、Codex
---

# 中断结束统计区间；计划正文从 CLI 自己的记录定位

## Context

真机全量走查发现：停止一轮后再发新请求，上一轮调用和 75 分钟间隔被计入新回复；Claude 2.1.322 首次 `ExitPlanMode {}` 没有内联计划，手机只有通用 Hook 审批。用户要求修复。决定要支撑的结果是新请求统计独立、首次计划可读；若修复破坏排队消息计数或审批状态，则不采用。

## Decision

采用现有宿主解析和索引作为唯一所有者，手机不补推断规则。

- 工作区间始于上一条 Agent 回复或中断，两者取较晚者。排队用户消息仍不能切断区间。使用已有 `turn=Interrupted` 索引记录，旧缓存直接适用；增加部分索引避免遍历整个会话。11-13 rebase 时 main 已由 #317（`2c5461d4`，索引 `session_records_stops`）按同一规则做完，这一半改用 main 的实现，本分支只留下它的回归测试。
- Claude 没带内联计划时，用原始日志的单段 `slug` 和该日志所属的配置目录定位 `plans/<slug>.md`。不把工具参数里的任意文件路径作为读取依据；读取上限 8192 字节。
- 计划尚未落盘时明确提示「看终端」，在既有 Write 结果及审批 Hook 到来时重读。路径保存在已有 pending call，退出审批或中断后不再恢复计划等待状态。沿用解析规则版本变更后的日志重读机制，不迁移或回填缓存。

## Consequences

聊天摘要、调用列表和最后调用共用中断边界。自定义 Claude 配置目录跟随原始日志路径。无 slug、无计划文件时保留明确的终端入口；不保证文件缺失时仍能展示正文。

## Implementation Plan

中断边界在 `log_index/chat.rs`、`log_index/tools.rs`、`content/db.rs`（#317）；计划正文在 `content/hook_question.rs`、`content/result.rs` 和 Claude Adapter。不要在手机按中文文案判断中断或再次寻找计划文件。

## Verification

- `cargo test -p fibo-host --lib`：旧索引、整页与分页首条回复、中断后的下一轮、排队消息、首次空计划、Write 后落盘、重启、审批 Hook、路径来源。
- `cargo test -p fibo-core --test guard --test file_size --test i18n`
- 真手机证据另存 `~/Documents/claude-pages/fibo/2026-11-11-phone-regressions-fixed/`，完整清单按 `docs/phone-app/walkthrough.md` 单独报告，不用单测代替。

## Alternatives

否定把每条用户消息都当作起点：会再次丢失排队前的调用。否定只读 `ExitPlanMode.input.plan`：当前真实 CLI 首次调用没有该字段。否定全局轮询计划目录：已有工具及 Hook 事件足够，不增加每帧工作或第二套缓存。

## More Information

来源是 2026-11-11 真机走查两项不过，原始截图和日志位于 `~/Documents/claude-pages/mooring/2026-11-11-phone-walk-2/`。新 CLI 改变 slug 或计划存储格式时，重新验证其实际首次审批，而非只测重试。
