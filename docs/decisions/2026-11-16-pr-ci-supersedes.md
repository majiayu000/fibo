---
status: accepted
date: 2026-11-16
decision-makers: majiayu000
---

# 同一 PR 的新提交取消旧轮 CI，main 的运行继续完成

## Context

`c3bf66c5`（2026-11-05）否定取消正在运行的 CI：多个会话频繁直推 main，会一直打断检查。11-06 起 main 必须走 PR；同一 PR 的旧提交已被新提交取代，继续占用 macOS runner 没有合入收益。用户审计后要求逐项优化。

## Decision

`ci.yml` 和 `ios.yml` 只在 `pull_request` 事件取消同组旧运行。main push 和手动运行仍完成。PR 按 ref 分组，因此不同 PR 互不取消；push 和手动运行按提交 sha 分组（2026-11-24 改）。这里只取代 `c3bf66c5` 对 PR 的选择，main 的理由和做法保持。

纯文档 PR 只在修改根 README/AGENTS/CLAUDE 或 `docs/` 下 Markdown 时省去两个原生 job；产品内的 skill Markdown、脚本、网站资产都继续检查。没有路径时，原生检查照常运行。路径判断或工作流回归失败时，必需 job 仍启动并明确失败，不让它以 skipped 通过。工作流仍产生同名必需 job；不使用 workflow 级路径过滤造成必需检查一直 pending。

## Consequences

被新提交取代的运行不会有最终结果，新提交重新验证。纯文档 PR 仍跑手机、relay 与工作流脚本回归。Linux job 在已有宿主测试产物上增加真实 TypeScript 手机客户端经 relay 发消息、读回和重连去重检查（本机先走通该用例；它仍不代替真手机）。无需改分支保护，也不改变 cargo-deny 规则。

## Implementation Plan

`.github/workflows/ci.yml`、`.github/workflows/ios.yml`。不把 fmt/deny 搬到新平台、不合并现有编译步骤。

## Verification

`actionlint`；`python3 -I scripts/test-ci-workflows.py` 验证文档、产品 skill、脚本、空集合、main、手动运行的选择。云端取消与 skipped check 的真实状态仍需首个 PR 验证。

## Alternatives

取消 main 正在运行的检查：保留原先否定，频繁合并仍会饿死结果。只靠后缀 `.md`：产品 skill 也是 Markdown，不能漏检。判断错误就跳过：必需 job 会被静默绕开，改成明确失败。

## More Information

[GitHub concurrency 文档](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency) 允许 `cancel-in-progress` 使用表达式。若 GitHub 的 PR merge checkout 语义变化，路径步骤会失败，并由必需 job 报错。

## 2026-11-24 更正：main 不能共用一个分组

`cancel-in-progress: false` 并没有让 main 的每个提交都跑完。GitHub 一个并发组只留一个等待中的运行，后到的会把等待中的那个取消掉。11-24 查最近 100 次 main 运行，31 次是 cancelled，全部 0 个 job，也就是那些合入提交从没被检查过（ruleset 的 strict=false，main 这一轮是唯一检查合并后代码的）。没被取消的运行也常先排约 10 分钟队（run 45345453672：总 23.1 分钟，前 9.6 分钟在等）。

改成 push 和手动运行按 `github.sha` 分组，PR 仍按 ref。代价是连续合并时会同时占用更多 macOS runner。

