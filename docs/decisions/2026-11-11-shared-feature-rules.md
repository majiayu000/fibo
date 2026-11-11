---
status: accepted
date: 2026-11-11
decision-makers: 用户、Codex
---

# 同一功能的规则由现有所有者提供

## Context

用户发现同一个运行圆圈在侧栏会转、标签页不转，随后要求检查并修复同功能的重复设计。审计确认至少六组；本次以 `c0892ee` 为起点，合入先前标签和窗格的修复。

决定要支撑的结果：开关在运行标记的各入口一致；手机不重新猜宿主已经知道的名称、能力、标题和额度等级。若所谓重复其实承担不同语义或不同平台，应保留。

## Decision

采用现有所有者，不建通用设置或状态框架：

| 功能 | 唯一实现 | 使用方 |
|---|---|---|
| 运行标记时钟、开关、减少动效 | `workbench/redraw/turning.rs` | 侧栏、标签、窗格、子 Agent、远程会话、未关联终端、看板栏头和抽屉 |
| 额度等级、窗口优先顺序 | core `quota::Window::level`、`Quota::by_usage/tightest` | 桌面直接调用；宿主把等级和排序下发给手机 |
| Agent 名称和排队/插入/发送队列/强度/下一句建议能力 | core Agent 注册表、宿主 Adapter 和 `agent_modes_switch` | 卡片、待办、额度、用量、新任务选择器；手机只展示下发结果 |
| 远程卡片最终标题 | 宿主 `remote/list.rs` | 桌面远程列表和手机 |
| 替换配置文件 | 现有 `ui/status_colors::write_atomic` | 机器、快捷键、侧栏、动效/提示、语言、风格、图标、更新检查、首次启动和额度置顶等选择 |
| POSIX 单参数转义 | platform `shell::quote_posix` | core 动作、终端路径、SSH 命令、Claude quota 包装脚本及 Unix 交互 shell |

保留 Windows 交互 shell 的独立转义；保留手机与桌面有意不同的状态排序、额度页面文案及样式。看板的等待光晕不是运行圆圈，仍按原来的效果绘制。抽屉盖住看板时，底下的栏头保留静态图标，避免共享覆盖层穿透遮罩。

## Consequences

- 额度 80% 开始警示、95% 开始严重；同百分比时保留来源顺序，两个客户端一致。
- 手机仍只展示账户额度，桌面保留按模型的额度窗口；这是既有展示范围差异。
- 新宿主从已有会话/识别状态生成字段，不改磁盘格式、不回填旧数据。现有配置内容与位置不变。
- 新手机连接旧宿主时：缺能力字段的操作不显示；Agent 用原始标识；缺最终标题显示终端 ID。旧额度缓存或旧宿主不带等级时，详情仍显示数值和中性色，概要不猜最高窗口。重新从新宿主读取后自然恢复。
- 旧手机/桌面可忽略附加字段；新宿主仍保留已有字段。额度窗口改变为按使用率排列。
- `send_now` 直接跟当前宿主允许的 `turn_keys` 一致，包含 Claude 和 Codex；未重新对真实 CLI 测量按键语义。

## Implementation Plan

已落实于上述模块和 `mobile/src/ui`、`mobile/src/remote/hub.ts`。不在手机重新加 Agent 白名单、阈值表或标题回退链；不放宽 guard/file-size 基线。

## Verification

- `cargo test -p mooring-core --test shared_quota --test guard --test file_size`
- `cargo test -p mooring-platform shell::tests`
- `cargo test -p mooring-host --bin mooring remote::list`
- `cargo clippy --workspace --all-targets --features mooring-app/visual-test -- -D warnings`
- `cd mobile && bun test && bun run typecheck`
- 原生 `frames-turning` 检查像素变化、侧栏缓存、栏头/抽屉、关闭开关和减少动效；完整 `scripts/perf-gate.py` 检查帧成本。
- 手机须按 `docs/phone-app/walkthrough.md` 在安装新构建后走完整清单；本地单测不能替代这一步。

## Alternatives

否定分别在每个入口补一套动画、在手机复制阈值和 Agent 名称表；这些正是此次分叉的原因。复查补齐另外七处同类配置落盘；否定把这些配置项改造成新的配置框架，现有原子写函数已经满足要求。

## More Information

范围是此次确认的六组，不声称全库不存在其他重复。下一句建议的相邻品牌判断也已移除，由 Adapter 是否提供屏幕读取函数决定。新的 Agent 能力或额度规则应先改拥有实际行为的宿主/核心实现，再下发给客户端。
