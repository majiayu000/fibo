---
status: accepted
date: 2026-11-10
decision-makers: lifcc
---

# 分隔条按两侧窗格确认身份

## Context

用户报告分隔条向下微拖反而向上，又要求继续检查并修复相邻问题。`a | (b | c)` 关闭 a 后，旧保存请求的路径和方向仍匹配，但它指向了另一条分隔条。完整比较子布局又过严：子比例更新会取消父分隔条的拖动。

要支撑的决定：后台怎样拒绝改错目标的保存，同时允许子布局的正常更新。
放弃条件：原分隔条消失后仍能写入，或只改变子比例就取消父拖动。

## Decision

采用按下时两侧各一个窗格 ID 作为锚点。后台按路径、方向及锚点所在侧确认目标；预览复用 core 的同一判定。子比例更新或在某一侧新增分屏可以保留，锚点关闭、换侧或分隔条消失则拒绝，用户重新拖动。

保存操作改名为 `ResizeDivider`，锚点字段必填；移除不带身份的 `ResizeSplit`，不做回退。只在旧名称上增加字段会被旧后台忽略，仍可能改错窗格。

既有布局文件格式不变。普通拖动仍限制在 5%～95%；已有比例超出该范围时，以初始比例扩展这一轮的边界，向内平滑调整，向外停在原处，不能一动就跳回 5% 或 95%。

## Consequences

新旧 App/后台混用时，分隔条保存请求会因未知操作而被拒绝解析，连接断开；旧后台不能返回新版错误提示，现有投递逻辑可能等待后提示结果未知，需同时更新后台。其他请求和已有布局文件不变。

关闭被选中的锚点时，即使还有其他窗格维持原来的视觉分隔，也会保守地取消这次拖动；重新按下会选取现存锚点。没有新增持久化的分隔条 ID。

## Implementation Plan

- `crates/mooring-core/src/layout.rs` 定义锚点与身份判定；`model/edit.rs`、`model/tabs.rs` 使用新保存操作。
- `crates/mooring-app/src/workbench/layout/resize.rs` 在按下时捕获身份，用于快照保留；`layout.rs` 原样带到保存请求。
- `crates/mooring-app/src/harness/split_evidence.rs` 的原生回放继续在必需 CI 检查内运行。

## Verification

- `cargo test -p mooring-app split_drag`
- `cargo test -p mooring-core --lib model::tabs`
- `cargo test -p mooring-host --test request_receipts layout_changes_from_two_hands_all_stay`
- `python3 scripts/resize-scenes.py target/debug target/resize-scenes`

覆盖父拖动期间子比例变化、替位分隔条不被旧请求修改、2%/98% 既有比例、旧请求被新后台拒绝。用旧后台进程额外验证新操作不被静默接收。

## Alternatives

否定整棵子树相等：子比例变化不是身份变化。否定只检查路径和方向：折叠后会命中其他分隔条。否定完整布局版本：无关编辑会阻断有效拖动。否定给布局节点增加持久 ID：此次目标用已有窗格身份即可满足。

## More Information

关联：PR #282；前一版 `510291b`。如果需要在锚点关闭后仍继续原拖动，再重新讨论节点身份；当前明确要求重新拖动。
