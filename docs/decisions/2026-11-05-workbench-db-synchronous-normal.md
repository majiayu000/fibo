---
status: accepted
date: 2026-11-05
decision-makers: majiayu000
---

# 工作台主库改 synchronous=NORMAL

## 背景

`workbench.sqlite` 原来是 WAL + `synchronous=FULL`，每次 commit 都在宿主全局锁里 fsync，约占后台剩余 CPU 的 10%（`docs/performance/index.html` 第 10 节）。当时的理由是主库存的是用户操作，所以记成「不改」。`docs/plan.md` 第 3 阶段已经定了改成 NORMAL，审计清单 C3 也点到锁内同步写库；用户要求按 plan 改。

## 决定

主库改成 WAL + `synchronous=NORMAL`，和日志索引一致。进程崩溃不丢数据；只有掉电或系统崩溃时，最后几笔事务（布局、待办、结局）可能丢。

不做：这次不把写库挪出锁、不加写库线程，那是 plan 第 3 阶段的另一项。

## 否掉的

- 保留 FULL：每笔都 fsync 且在锁内，换来的只是掉电时多保住最后几笔。
- 改 OFF：进程崩溃之外，系统崩溃可能损坏库，不接受。

## 验证

`cargo test -p mooring-host --lib store::tests::the_workbench_db_commits_with_synchronous_normal`

## 重开条件

用户报告掉电后丢了确实不能丢的操作，再按表区分持久性。
