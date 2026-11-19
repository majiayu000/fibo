---
status: accepted
date: 2026-11-19
decision-makers: maintainer
---

# 本轮计时扣掉等你的时间：等待到你回答为止，回答由后台看到的输入定

## 背景

待办 3636b388：Agent 提问后三小时才答，接着跑的这一轮计时把三小时也算进去。#427 第一版在轮次离开「等你」时扣掉 `signal_at` 到下一个信号之间的时长。

审查发现它把工作也扣掉了：Claude 在你批准工具的那一刻不发信号，下一个信号是工具跑完后的 PostToolUse。批准后 `cargo build` 跑十分钟，这十分钟被当成等待，工具一结束，侧栏、看板、手机的计时往回跳十分钟。

## 决定

一次等待的终点是你回答的时刻。Fibo 终端里的回答都经过后台的输入（窗口敲键、手机点允许或回答、`fibo send`、CLI 接口的审批），后台在 `Host::answer_screen_wait` 里把这一刻记到会话卡片的 `Waited::answered`（等待中第一次输入），轮次离开等待时只扣「等待开始 → 回答」这一段。

后台没看到回答（会话不在 Fibo 终端里跑）时，不扣，计时和 #427 之前一样。

不做：按画面判断「确认框已答、计时行重新出现」来定终点；按信号种类区分提问和审批来决定扣不扣。

## 后果

- 提问、审批、计划，只要在 Fibo 里答，都按回答时刻扣，三种 Agent 一样，不依赖哪家 CLI 的界面。
- 不在 Fibo 终端里的会话，等待仍算进计时（老行为）。
- 等待中按方向键挑选项也算开始回答：等待提前几秒结束。
- 旧卡片没有 `answered`，读作 0：升级时正在等的那一轮，若升级后才答，答的时刻会被记下并扣除；升级前答完的不补算。

## 实现

- `crates/fibo-core/src/content/waited.rs`：`Waited::answered`、`Content::answer`、`end_wait`。`content.rs` 已到 800 行，新字段放在 `Waited` 里。
- `crates/fibo-host/src/host/mod.rs`：`note_answer` 只在卡片在等且这次等待还没记过回答时写一次（`Index::answered`，然后把卡片放回工作区）。
- 别在 App 或手机里各自扣等待：计时起点只由 core 的 `Content::clock_from` 给。

## 验证

```sh
cargo test -p fibo-core --lib waited
cargo test -p fibo-host --lib time_tests
cargo test -p fibo-host --test managed_session answer_clock
```

## 否掉的

- A：只对提问和计划扣，审批不扣。审批挂三小时仍算进计时，原来的抱怨只解决一半。
- B：看画面上确认框消失、计时行重现的时刻。又一条认画面的规则，CLI 改界面就会认错，且结果写进存下的数据；这几天已修过折行审批、Ultra 输入框符号等同类问题。
- 没看到回答时按种类退回 A：要在每处进入等待的地方记下种类，`choices` 在等待结束前就被清掉；为少数场景加的复杂度不值。

## 更多

- 重开条件：发现大量会话在 Fibo 终端外运行且等待很长；或某个 Agent 的回答不经后台输入（例如它自带网页审批）。
- 来源：PR #427（6b4eec09）、#439（回答时刻 `screen_answered`），本会话 11-19 用户选 C：「哪个最长远呢？」
