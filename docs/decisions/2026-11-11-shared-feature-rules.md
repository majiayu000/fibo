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

- `cargo test -p fibo-core --test shared_quota --test guard --test file_size`
- `cargo test -p fibo-platform shell::tests`
- `cargo test -p fibo-host --bin mooring remote::list`
- `cargo clippy --workspace --all-targets --features fibo-app/visual-test -- -D warnings`
- `cd mobile && bun test && bun run typecheck`
- 原生 `frames-turning` 检查像素变化、侧栏缓存、栏头/抽屉、关闭开关和减少动效；完整 `scripts/perf-gate.py` 检查帧成本。
- 手机须按 `docs/phone-app/walkthrough.md` 在安装新构建后走完整清单；本地单测不能替代这一步。

## Alternatives

否定分别在每个入口补一套动画、在手机复制阈值和 Agent 名称表；这些正是此次分叉的原因。复查补齐另外七处同类配置落盘；否定把这些配置项改造成新的配置框架，现有原子写函数已经满足要求。

## More Information

范围是此次确认的六组，不声称全库不存在其他重复。下一句建议的相邻品牌判断也已移除，由 Adapter 是否提供屏幕读取函数决定。新的 Agent 能力或额度规则应先改拥有实际行为的宿主/核心实现，再下发给客户端。


## 2026-11-11 追加检查与修正

上一轮漏了行为边界：额度严重程度相同，不代表过期后仍应展示旧百分比；收尾结果除了 done、deferred 还有 abandoned。

- 采用：手机额度详情和概要共用 `quotaReset`，到重置时间后详情显示「已重置 / 等下次使用时更新」并清空量表；复用既有 30 秒时钟，离线缓存也会跨过重置边界。各 Agent 单独标注读数时间，避免用同机其他 Agent 的最新时间掩盖旧读数。不改缓存格式，旧缓存缺少严重程度时仍可正确显示重置。
- 采用：无新请求的 abandoned 不进手机收件箱；有新请求按既有规则回来。撤销恢复 abandoned，不能变成 open。宿主的现有 outcome 接口已支持 abandoned，不新增操作。
- 采用：看板待处理光环和运行卡片扫光也遵循 core 的减少动态效果标志。保留 elapsed time 的时钟，不在 `pace::wake` 全局禁止定时器；减少动态效果不能让本轮时长停止更新。
- 采用：Claude 菜单和日志共用 `permission::claude` 文案映射；菜单 manual 与日志 default 表达同一模式，dontAsk 不再遗漏。
- 待验证：未关联会话但已识别的 Agent 是否应出现在手机，涉及产品展示范围，本轮不改变。权限标签以文案判断全放行仍是维护耦合，未知菜单项的保守确认策略与卡片的已知权限展示不应混为一项已复现故障。读数过旧时的灰显策略尚未在手机收敛，不能把本轮额度修正说成所有新鲜度策略都统一。

验证补充：手机 `inbox.test.ts` 验证收尾与撤销，`parse.test.ts` 验证重置边界、未知重置时间与旧缓存；host `claude_log_and_menu_share_permission_words` 验证日志和菜单文案。原生 `frames-board-motion` 独立检查活跃窗口下光环/扫光的像素检查，同时验证普通动效确实会动，避免在非活跃窗口假通过。真手机验收独立于这些检查。


## 2026-11-11 剩余项继续修复

用户明确要求「修复啊，继续修」。采用并落实上一轮留下的三项：

- 手机收件箱保留 `agent` 已识别、`session` 尚未关联的终端，展示宿主的最终标题/识别说明；普通 shell 仍隐藏。复用现有行和终端入口，没有会话的行不提供收尾滑动操作；电脑页的“对话数”仍只统计真正的会话。
- 读数新鲜度由 core `Quota::fresh_until/stale` 决定；宿主下发截止时间，手机详情和概要都依此灰显，并复用已有时钟跨过离线边界。缺少截止时间的旧缓存作为未知/历史读数展示，不在手机复制一小时常量或补写缓存。
- 解析器从 Claude 原始 permissionMode、Codex 原始 approval_policy/sandbox_policy 生成 `permission_full`；模式切换也记录该语义。Content、临时切换结果、远程 Card 和手机附件面板/列表传递同一个标志，文案只负责显示。新日志取代临时模式时，标志与文案一起取代；旧快照缺字段和未知模式都是未知，不从中文倒推。未知 Codex 菜单仍按既有策略要求确认，但不冒充已知 full 状态。

性能调查的采用项：打开“通用”设置不预读插件目录和展示文件，进入插件页或搜索命中插件页时才读。原生分段测量中，预读约 4.79 M，占设置初始化 4.91 M 的约 97%。复用 Market 已有初始化状态，没有增加缓存层或配置；插件页首次读取成本另记为 `settings-plugins-first-frame`（含排版）。`frames-plugins` 断言默认页未读、打开插件页已读，继续验证动画卡片和三种主题。

否定只在手机把中文判断搬到宿主：那仍然让文案决定权限语义。否定重跑至性能变绿和放宽基线；删除确定的多余预读，并保留原生反向验证。兼容边界仅使用现有 serde 的缺字段语义，没有磁盘迁移、回填或旧文案解析。

验收：手机的 unbound/inbox、quota freshness、bilingual permission flag；host 的原始日志解析、模式切换覆盖、远程序列化和日志更新；core 的新鲜度边界；原生插件按需加载及性能门禁。真手机与已安装 App 的版本验收仍需与源码验证区分。

验收补充：撤回未关联 Agent 的过滤修复、新鲜度规则或插件延迟读取后，相应检查均失败。撤回看板减少动效修复后，原生探针在 reduced=true 时仍观察到 2706 个变化像素，补上前轮缺少的旧版反向证据。当前前台复验仍可能因 OS 未给测试窗口焦点而中止，不把这种中止当作通过；常规性能门禁独立运行。设置热打开约 0.0593 M，单项基线已从 4.92 收紧为 0.06 M。
