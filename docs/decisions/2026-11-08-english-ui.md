---
status: accepted
date: 2026-11-08
decision-makers: majiayu000
---

# 界面英文化：中文原文当键，英文表编进程序

## Context

公开发布用英文 README 和网站，但窗口里每个字都是写死的中文（`crates/fibo-app/src` 约 2300 个中文字面量，加上 core 的状态词）。外国用户要看到英文，中文用户要看到和今天一模一样的界面。翻译要分几批、由不同 Agent 并行做完，所以机制必须能机械地套用、进度可量。

## Decision

- 代码里的中文原文就是键：`t!("新建标签")`；带参数 `t!("共 {n} 个会话", n)`，英文可写 `{n|session|sessions}` 按单复数选。`const` 表和 match 里用 `tk!("…")` 标出键、显示处 `i18n::say(…)`；不是给人看的中文（读屏幕匹配、写文件、协议）用 `not_ui!("…")`。机制在 `crates/fibo-core/src/i18n.rs`。
- 英文在 `crates/fibo-core/locales/en/*.toml`（`"中文" = "English"`），build.rs 自动收所有表编进程序；新增一张表就是新增一个文件，并行的人各写各的表。
- 语言：设置 › 通用 › 语言，跟随系统（默认：系统首选语言是 zh* 用中文，其他用英文）、中文、English，存 `~/.config/mooring/language`；`MOORING_LANG` 临时覆盖（截图用）。系统读不到时用中文。
- 只有窗口设语言。宿主、`mooring` 命令（Agent 读的输出）、插件校验报错仍是中文，不在这次范围。
- 同一中文要两种英文时（断开：按钮是 Disconnect，状态是 Offline），少见的那处写 `t!("断开#button")`：`#` 后跟小写英文是上下文，中文显示时去掉。地方窄放不下整词时同样用上下文（`Status::short_label` 的 `等你回答#short` = Waiting）。
- 不做：其他语言（表结构允许以后加 `locales/<lang>/`，现在没有需求）。

## Consequences

- 中文界面不变：中文模式下 `t!` 原样返回字面量，测试和截图默认都是中文。
- 缺英文的键回落到中文，`tests/i18n.rs` 会失败；表里多出没人用的键也失败。
- 切换语言后窗口立即重画；启动时读一次的文字（主题名、macOS 菜单栏）要重开才换。这一点只看过代码，没实际切换验证。

## Implementation Plan

转一个文件：

1. `python3 scripts/i18n-wrap.py <文件>`：包 `t!`、把简单的 `format!` 改成带名字的 `t!`，打印跳过的地方和缺英文的键（`"键" = ""`）。
2. 看 diff：`const`/`match` 改 `tk!`；拿来比较或保存的中文还原或 `not_ui!`；工具没改的 `format!` 手改成整句键（英文语序不同，别拼碎片）。
3. 英文写进自己的 `locales/en/<区域>.toml`；术语照 `core.toml` 头部和已有表（同一个键在几张表里出现时英文必须一样，先 `grep -rn '^"键"' crates/fibo-core/locales/en/`）。按钮、标签、列头这类地方英文控制在一到三个词，长解释放提示行。
   - 元素 id 和测试、菜单动作按中文找的键保持中文（`not_ui!` 或 `tk!` + 显示处 `say`）。
   - 状态词不手写，用 `Status::X.label()` 作 `t!` 的参数（`tests/guard.rs`）。
   - 带 ⌘ 和「访达」的文字照旧交给 `ui::words::local(t!(…))`，英文写 Finder，换系统时由 words.rs 改说法。
   - 日志、panic、`expect` 里的中文不算界面，不用转。
4. `cargo test -p fibo-core --test i18n`；数字下降后 `MOORING_GUARD_UPDATE=1 cargo test -p fibo-core --test i18n` 降基线。
5. 截图：`MOORING_LANG=en MOORING_PAGE_LIST=… python3 scripts/visual-fixture.py capture <目录>`，看英文有没有截断、挤压。

## Verification

- `cargo test -p fibo-core --test i18n`：每个键有英文、占位符一致、各表不冲突；未转的中文字面量只降不升（`tests/i18n-baseline.txt`）。
- `cargo test -p fibo-core --lib i18n`：跟随系统的判定、占位符和单复数、表格式。

## Alternatives

- 英文 key（`t!("tab.new")`）加中英两张表：中文界面要改动每一处的来源，容易把今天的中文改走样；代码里也看不出显示什么。
- fluent / gettext 一类库：多一个依赖和一种格式，复数和参数这里只用到最简单的一层。
- 一张大表：并行翻译时人人改同一个文件，合并冲突。
- 运行时读外部文件：打包要带上，缺文件时静默回落，测试管不住。

## More Information

来源会话：2026-11-08 英文发布前的 i18n 第一期。
