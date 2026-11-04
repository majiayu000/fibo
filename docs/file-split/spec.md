# 大文件切分：搬之前先定归属

状态（2026-11-05）：设计稿。是 [README.md](README.md) 的补充，不是第二份清单。

- [README.md](README.md) 管**怎么搬**：30 个文件逐个怎么切、批次、陷阱、六层验收。它要求拆分提交只搬不改。
- 本文管**搬之前要改的归属**：有几处如果原样搬过去，会把重复规则和上帝对象一起搬走。这些改动会改代码，按 README 的规矩不能混进 `refactor(split):` 提交，所以单独列在这里，各开一个 PR。

逐文件的切法以 README 为准，本文不再重复。

## 要支撑的决定

README 批次 4（窗口界面）开工前，下面两件事要先做完，否则批次 4 会把问题固化：

1. 「状态 → 分组」这条规则在桌面写了三遍、手机写了一遍，宿主给远程卡片用的空间算法又不一样（审计 A2、A3）。
2. `Pane` 有 65 个字段、约 90 个方法。按 README 把方法分到几个文件后，每个文件仍然能读写全部 65 个字段：文件拆开了，职责没有拆开。`impl Workbench` 分布在 46 个文件里就是这个结果。

## 原则

1. **按「谁拥有这个事实」划分。** core 判定事实和派生规则（纯函数，无 I/O）；host 采集证据、做 I/O、持锁；app 和手机只负责显示和交互状态。一条规则有两个界面在用，就进 core（`docs/plan.md` 第 7 条的延伸）。
2. **先分字段，后分文件。** 一个类型要拆到多个文件，先把字段按关注点收进子结构，每个文件只碰自己那组字段。
3. **不加层、不加 trait、不加 crate。** 只用模块和子结构。
4. **改归属和纯搬移分开提交**，前者带测试，后者走 README 的审计。

## 归属调整（各一个 PR，改代码，不用 `refactor(split):` 前缀）

### G1 · 分组规则收进 core（README 批次 4 之前；修 A2、A3）

- 新增 core 的分组函数（`status.rs` 当前 574 行，放进新文件 `crates/mooring-core/src/status_group.rs`），覆盖看板列（要你处理 / 在跑 / 停下了 / 很久没动 / 做完了）、侧栏桶、侧栏排序层级。
- 以下几处改为调用这个函数：`workbench/board.rs` 的 `column_of`、`workbench/board/remote.rs` 的 `remote_column`、`workbench/sidebar_list.rs` 的 `bucket`、`workbench/sidebar.rs` 的 `sidebar_rank`。删掉 `sidebar.rs` 自己那份 `NEEDS_YOU` 常量，它漏了「等后台」。
- `app/src/model/session_state.rs` 的 `session_space` 移到 core 的 `Workspace`。`host/src/remote.rs` 组远程卡片时改用它，不再用终端的 `t.space`。
- 远程协议的 `Card` 增加算好的 `column` 字段。手机 `mobile/src/app/board.tsx` 和桌面 `remote_column` 只读这个字段，同时删掉 `mobile/src/ui/status.ts` 里按 `phase` 猜状态的兜底。
- `crates/mooring-core/tests/one_owner.rs` 增加对照：对 `sample::every_state()`，本机看板列、远程卡片的 `column`、侧栏桶三者一致。

### G2 · `Pane` 字段分组（README 批次 4 拆 `pane.rs` 之前）

只把字段收进子结构，不搬文件，不改行为：

| 子结构 | 收进去的状态 | 用到它的方法 |
|---|---|---|
| `Selection` | 选区、拖选、自动滚动 | `drag_selection`、`autoscroll`、`cell_at`、`select_all`、`clear_selection`、`copy` |
| `AskPin` | 提问定位和固定 | `set_asks`、`pinned_latest`、`back_to_ask`、`locate_asks`、`ask_overlays` |
| `Search` | 搜索框和命中 | `open_search` … `find_text` |
| `Links` | 悬停词、路径识别 | `hover_word`、`check_hover_path`、`open_target`、`click_hint` |
| `Handoff` | `Stage`、`Handoff` | `retry/dismiss/step/watch_handoff`、`prefill` |

做完后，README 给 `pane.rs` 写的切法（`Render`、`EntityInputHandler`、`key`、`ask_overlays` 分开）照常执行，各文件就只碰各自的子结构。

### G3 · 截图探针整模块 gate（README 批次 5 之前，可选）

`workbench/visual.rs` 里有 85 个逐项的 `#[cfg(feature = "visual-test")]`，`pane.rs` 里有 26 个 `visual_*` 方法。改成在模块声明处 gate 一次：`#[cfg(feature = "visual-test")] mod visual;`，`pane` 的探针移进 `pane/visual.rs`。这样能消掉 README 陷阱 4 的一部分：漏开特性也不会悄悄编进发布版。

## 顺序

```
README 批次 1–3（只搬不改，可先做）
G1 ──┐
G2 ──┼─ README 批次 4（窗口界面）
G3 ──┴─ README 批次 5（验收工具）
```

审计清单里的其它修复（OSC 7501、忙时丢弃、静默吞错、文档漂移、tokio）先合入，G1–G3 基于它们之后的 main 来做。

## 不做

- 不新建 crate，不引入 trait 或「服务层」。
- 不为凑 800 行切开一个内聚的函数。
- 审计 C1–C3（锁毒化、工作区只增不减、热路径上的 ps）是行为修复，各自另开 PR，不并进切分。
