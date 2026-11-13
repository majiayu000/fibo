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
- **显示净改动。** 同一文件的多次编辑合成一份（`fibo_core::turn_diff`），改过两次的行只出现一次，改了又改回去的不出现。行号和已知内容对不上说明中途有别的程序写过文件，这时不合成，按次显示并写明。
- **点开时才读，不进工作区快照。** 快照每次变化都整份发给窗口，改动内容放进去会超过同步开销 1% 的放弃线（`docs/performance`）。
- **打开只取文件列表，选中才取内容。** 列表带每个文件的编辑在日志里的位置，选中时直接读那几行，不重新扫日志。5000 个文件时只有屏幕上那个文件花钱，实测点一个文件约 32 ms（调试版）。第一版一次取回全部内容，64 个文件各 2000 行就是 15 MB，已否定。
- **数字必须准确。** 会话卡片只列 64 个文件（列表会进快照），但这一轮被挤掉的文件和行数单独记下，回合条写真实总数。第一版超过 64 个文件时悄悄显示 64，比卡更糟，已否定。
- **盖在终端上，不推开终端。** 终端尺寸不变，程序不用重排。用方向 A：盖 56%，留出 Agent 最后的话和输入行；打开时输入行在上半部分就盖下半部分。
- **只读。** 不做批注和发回 Agent，11-05 已否定，意见在终端里打。

## Consequences

- 用命令或脚本改的文件不在列表里，面板底部写明。
- 回合条的行数是每次编辑累加，面板是净结果，改过多次的文件两边不一样；面板标题写「编辑 N 次，这里是合并后的结果」。
- 只接了 Claude 和 Codex 的记录，其他 Agent（Grok、Cursor、Qoder、OpenCode 等）显示「只有行数」。
- 一次最多读 3 秒记录，没读到的文件显示「只有行数」。

- 文件很多时左列只画可见行，超过 20 个文件出现过滤框（「在终端里查找」的快捷键，默认 ⌘F；不区分大小写，空格分隔的词都要匹配）。
- 后台第一次发现一个已经很大的日志时只读尾部，那时回合条的总数可能偏小；面板的列表读的是日志本身，不受影响。之后都是增量读取，总数准确。

## Implementation Plan

受影响：`crates/fibo-core/src/turn_diff.rs`、`crates/fibo-host/src/agents/turn_edits.rs`、`crates/fibo-host/src/host/client/turn_changes.rs`、`crates/fibo-app/src/workbench/layout/changes_panel.rs`、`changes_files.rs`、`changes_rows.rs`、`changes_slots.rs`、`crates/fibo-core/src/content.rs`（`Evicted`）、`scripts/perf-gate.py`。接新的 Agent：实现 `edit_marker` 和 `logged_edits`。

## Verification

- `cargo test -p fibo-core turn_diff`：含 400 轮随机编辑序列，合成结果必须把原文件变成最终文件。
- 真实窗口：`MOORING_CHANGES_EVIDENCE=1 fibo-app --visual-evidence <目录>`（需 `visual-test`），逐条走状态表。
- `scripts/perf-gate.py` 的 `frames-changes` 和 `frames-changesdraw`（都是 5000 个文件）：终端输出时面板不重画，面板自己重画的开销不随文件数涨，都不超基线 15%。

## Alternatives

- 接手机的 `git diff`：见 Context 和上面第一条。
- 把补丁存进 `Content`：见「点开时才读」。
- 编辑时给文件拍快照，轮末算真 diff：最准，也能覆盖脚本改动，但每次编辑都要读写文件，还要管存储，为了少见情况付常驻成本。
- 方向 B（盖满）：看不到 Agent 说的话。方向 C（右侧抽屉）：挡住终端右半，打长句看不全。
