---
status: accepted
date: 2026-11-24
decision-makers: Fibo maintainers
---

# 本机诊断按需检查工作区和无窗格终端

一台开发机测到已合入、工作区干净的 Rust 编译产物合计 556 GB。逐个遍历会占用几分钟。诊断的应用排行按程序归类，同名 Agent 或同一 App 包内的几个进程可能合并显示。

## 决定

- **#561 采用。** 从已打开的空间、受管终端目录和空间的最近目录找到 Git 仓库。用本地 `origin/HEAD` 的祖先关系判定合入。Squash 合入和缺少上游默认引用的仓库无法由这项检查确认。扫描显式开始，关闭面板停止本次扫描，已完成的结果保留在窗口会话内。清理前重新核对 Git、终端、进程和空间。清理产物限有 Cargo 缓存标记且被 Git 忽略的 `target/`。完整移除也保护其他忽略文件和已设置的空间目录。旧会话在原目录已消失时沿用启动目录回退。
- **#562 采用。** 后台从本地 socket 取窗口的进程身份，从 PTY 根和进程树汇总，避免同一个进程落入两段。旧后台或无法确认请求来自本机窗口时隐藏四段。
- **#563 采用。** 四段图回答 Fibo 自身、Agent 和普通终端的总内存。识别同名 Agent 中哪一个终端在消耗 CPU 仍需要按空间展开。终端 CPU 与按应用排行来自同一组进程采样，旧后台无数值时显示「—」。
- **#564 采用。** 现有侧栏的「未关联」指有窗格但没有会话。绑定了会话的终端有机会从会话入口恢复窗格，普通 shell 没有对应 GUI 入口。移动端开终端和直接发 Launch 仍能在布局提交前留下 PTY。诊断列出运行中且没有窗格的终端，后台在放置或结束前再次查布局。

## 验证

- `cargo test -p fibo-host --test managed_session worktree_cleanup` 在隔离仓库比较实际磁盘占用，检查两档清理、脏工作区、未合入、进程占用、忽略文件、空间目录与旧启动目录。
- `cargo test -p fibo-host --test managed_session diagnostic_stream_measures_a_busy_managed_terminal` 用真 PTY 检查诊断能识别无窗格终端、计算 CPU，以及放置和结束。已放进窗格的终端必须拒绝从旧诊断结果结束。
- `cargo test -p fibo-core --lib legacy_report_has_no_owner_chart_or_agent_identity` 验证旧后台的报告。

如果多台机器的实际可回收量普遍低于 100 GB，可以缩减为只读清单。若所有 Launch 路径都能在一个事务里完成放置，且重复实测无遗留，可删除 #564 的入口并保留回归检查。
