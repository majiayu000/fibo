# 窗口导航收成一个入口：当前是哪一页只有一个答案

状态：方案，待用户拍板，未改代码。对应 [#615](https://github.com/majiayu000/fibo/issues/615)；它是 [#614](https://github.com/majiayu000/fibo/issues/614)（逐个验收页吞掉点击和 ⌘J）的根因（11-26 窗口层审查，汇总在 #621）。

## 要支撑的决定

- 主区现在显示哪一页（某个标签、鸟瞰、逐个清理 / 验收的队列、待办详情、表单），由谁决定、在哪里改。
- 指标：每个跳转入口（左栏点击、⌘J、通知、标签键、鸟瞰按钮、⌘⇧O、表单的开和关、队列里的动作）走同一条路，不会再出现「点了没反应」；「是否在鸟瞰或队列」只算一次。

## 现状（2026-11-28，main fc4c954b）

### 状态散在几处

- `workbench/nav.rs` 的 `Nav`：`space`、`tab`、`focused`、`pending_focus`、`zoomed`、`overview`（bool）、`task_detail`、`trail`（「回到刚才」，#515）。已有一个雏形 `type Page = (空间, 标签, 是否鸟瞰, 待办详情)`，只给 `trail` 用。
- 不在 `Nav` 里、却决定主区显示什么的：队列（`overview.cleanup`、`cleanup_index`、`cleanup_review`、`review_rest`）、表单（`tasks.form`）、另一台电脑的工作台（`hosts`）。设置、诊断、⌘P、偷看、弹出菜单是盖在上面的层。
- 非测试代码里有 18 处直接给 `nav.overview` 赋值。

### 同一个判断写了 4 份，有一处不一样

- 「现在是否在鸟瞰或队列」写成 `self.nav.overview || !self.overview.cleanup.is_empty()`，出现在 `render.rs`（标题、鸟瞰按钮两处）、`render/keys.rs`（标签键）、`render/close_key.rs`（关闭键）。
- 鸟瞰按钮把这个结果传给 `toggle_overview`；⌘⇧O（`render/keys.rs` 的 `Action::Overview`）只传 `self.nav.overview`。从队列打开「稍后再看」表单时，`forms.rs` 的 `show_form` 把 `nav.overview` 设成 false，这时按 ⌘⇧O 会进鸟瞰，点同一位置的按钮却是退出鸟瞰。

### render 里改状态

- `render.rs` 每次 render 开头先做 6 件带副作用的整理（`settle_hosts`、`follow_space_theme`、`settle_changes`、`before_render`、`settle_board_transients`、`return_to_queue`）；`redraw.rs` 的注释也承认，render 里发出的 notify 到不了侧栏，只能等下一帧补。
- `return_to_queue`（`review_queue.rs`）每帧把 `nav.overview` 拉回 true：本意只为「从队列打开的表单关掉之后回到队列」，结果把左栏点击和 ⌘J 也拉了回去（#614）。

## 可选方案

### A 一个 `Page` 枚举加一个 `go`，规则写成纯函数（推荐）

- 主区只有一个答案：`Page::{ Tab { space, tab }, Board, Queue { review }, TaskDetail(id), Form { kind, back_to } }`。设置、诊断、⌘P、偷看、菜单仍然是盖在上面的层，不进 `Page`。
- 跳转规则写成纯函数：当前页加一个事件（点了会话、⌘J、通知、表单关了……），得出下一页和要做的事（离开队列时保留「接着验收」的剩余部分、要不要 `sync_panes`、焦点给谁、要不要记进 `trail`）。`Workbench` 的 `go` 只负责执行这些事。
- 表单带 `back_to`：从队列打开的表单关掉就回队列，不再需要每帧的 `return_to_queue`。
- 4 份判定换成 `Page` 上的一个方法；⌘⇧O 和按钮自然一致。
- 好处：规则能脱离窗口写单测（`Workbench` 本身起不来，见 #613），每个入口一行 `go(…)`；`trail` 直接存 `Page`。

### B 只收判定、修入口，不引入 `Page`

- 把 4 份判定收成一个函数，⌘⇧O 跟着改；在 `focus_terminal`、`open_session` 里离开队列（#614 的小修）。
- 改动小，但下一个入口照样要记得那一整套，同类问题还会再出。

## 推荐

先 B 止血，再 A：

1. 小 PR：4 份判定收成一个函数，⌘⇧O 改用它；`focus_terminal`、`open_session` 开头队列非空时调 `leave_for_one`（修 #614）。守卫用 `review_queue_evidence.rs` 的原生场景，加一步「队列进行中打开一个不在队列里的会话」，再用 `visual-check.py guard … --undo` 确认撤掉修复后这一步失败。
2. 引入 `Page` 和纯函数规则，`go` 接管 18 处赋值和全部入口；表单带 `back_to`，删掉 `return_to_queue`。
3. 守卫：加一个计数测试，`nav.overview`（之后是 `Page`）只能在 `nav.rs` 里改，别处新增直接赋值就失败（仿 `crates/fibo-core/tests/guard.rs` 的棘轮写法）。

## 接口和改动的文件（预估）

- `crates/fibo-app/src/workbench/nav.rs`：`Page` 枚举、规则纯函数、`go`；新代码放新文件，守住 800 行。
- 入口：`sessions.rs`（`focus_terminal`、`enter_card`、`navigate_to`）、`resume.rs`（`open_session`）、`render/keys.rs`、`render/close_key.rs`、`render.rs`、`overview.rs`（`toggle_overview`、`leave_overview`）、`forms.rs`（`show_form`）、`review_queue.rs`、`cleanup.rs`、`nav/back.rs`（「回到刚才」）。
- 不动：另一台电脑的工作台（`hosts`）有自己的一套，嵌在这一页里；偷看、菜单、设置照旧是层。

## 验收标准

- 逐个验收页上点左栏会话、按 ⌘J：离开队列，打开那个会话；「接着验收」能回到原来的位置。
- 从队列打开「稍后再看」表单，保存或取消后回到队列；表单开着时按 ⌘⇧O 和点鸟瞰按钮效果一样。
- 规则纯函数的单测覆盖每个入口事件在每种当前页下的结果。
- 「回到刚才」（#515）、通知跳转、另一台电脑的页面行为不变：现有原生场景（`focus_jump_evidence`、`review_queue_evidence` 等）全过。
- render 里不再改 `nav.overview`。

## 放弃条件

- 第 2 步发现另一台电脑的页面、偷看等无法用「页 + 层」表达，且硬塞会让规则失去纯函数的性质时：只做第 1、3 步。

## 不做

- 把 `settle_hosts`、`settle_changes` 等其余 render 里的整理一并挪走：先只拿掉跟页面有关的 `return_to_queue`，其余另议。
- 改界面长什么样。

## 要你拍板的问题

1. 第 1 步（止血，修 #614）先单独发，可以吗？
2. 表单算一页（推荐，带 `back_to`），还是算盖在上面的层？
3. 守卫的棘轮从现在的 18 处开始只降不升，还是第 2 步做完直接要求 0？
