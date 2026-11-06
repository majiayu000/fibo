---
status: accepted
date: 2026-11-06
decision-makers: majiayu000
---

# 看改动从会话记录取，合成净改动，盖在终端上

## Context

回合条的「看改动」原来只展开文件名和行数，用户说它「好像没有实际作用」。手机已有 `git diff` 实现，看上去可以直接接。

## Decision

- **内容来自 Agent 的会话记录，不用 git。** 用户截图那一轮的目录不是 git 仓库，用 git 一行都拿不到。这一轮已提交的改动也看不到，还会混进别的会话的改动。
- **显示净改动。** 同一文件的多次编辑合成一份（`mooring_core::turn_diff`），改过两次的行只出现一次，改了又改回去的不出现。行号和已知内容对不上说明中途有别的程序写过文件，这时不合成，按次显示并写明。
- **点开时才读，不进工作区快照。** 快照每次变化都整份发给窗口，改动内容放进去会超过同步开销 1% 的放弃线（`docs/performance`）。
- **盖在终端上，不推开终端。** 终端尺寸不变，程序不用重排。用方向 A：盖 56%，留出 Agent 最后的话和输入行；打开时输入行在上半部分就盖下半部分。
- **只读。** 不做批注和发回 Agent，11-05 已否定，意见在终端里打。

## Consequences

- 用命令或脚本改的文件不在列表里，面板底部写明。
- 回合条的行数是每次编辑累加，面板是净结果，改过多次的文件两边不一样；面板标题写「编辑 N 次，这里是合并后的结果」。
- 只接了 Claude 和 Codex 的记录，其他 Agent（Grok、Cursor、Qoder、OpenCode 等）显示「只有行数」。
- 一次最多读 3 秒记录，没读到的文件显示「只有行数」。

## Implementation Plan

受影响：`crates/mooring-core/src/turn_diff.rs`、`crates/mooring-host/src/agents/turn_edits.rs`、`crates/mooring-host/src/host/client/turn_changes.rs`、`crates/mooring-app/src/workbench/layout/changes_panel.rs`、`changes_slots.rs`、`scripts/perf-gate.py`。接新的 Agent：实现 `edit_marker` 和 `logged_edits`。

## Verification

- `cargo test -p mooring-core turn_diff`：含 400 轮随机编辑序列，合成结果必须把原文件变成最终文件。
- 真实窗口：`MOORING_CHANGES_EVIDENCE=1 mooring-app --visual-evidence <目录>`（需 `visual-test`），逐条走状态表。
- `scripts/perf-gate.py` 的 `frames-changes`：终端输出时面板不重画，帧开销不超基线 15%。

## Alternatives

- 接手机的 `git diff`：见 Context 和上面第一条。
- 把补丁存进 `Content`：见「点开时才读」。
- 编辑时给文件拍快照，轮末算真 diff：最准，也能覆盖脚本改动，但每次编辑都要读写文件，还要管存储，为了少见情况付常驻成本。
- 方向 B（盖满）：看不到 Agent 说的话。方向 C（右侧抽屉）：挡住终端右半，打长句看不全。
