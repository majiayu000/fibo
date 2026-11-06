---
status: accepted
date: 2026-11-06
decision-makers: majiayu000
---

# main 必须走 PR 且 CI 通过

## Context

`main` 没有服务端保护，规则也写着可以直接 push、不用等 CI。2026-11-06 `0638fe9` 直接 push 后 main 变红，其他 PR 照常合入，用户问：「怎么会有一个红的CI进入合并呢」。边界测试当天第二次因为直接 push 变红。

## Decision

Ruleset `main requires CI` 作用于 `main`：必须走 PR，`Workbench Rust checks` 通过才能合入，不能强推和删除，没有任何例外（管理员也一样，Agent 用的就是管理员账号）。

不做：要求基于最新 main；要求人工批准；把另外三个 job 设为必需。

`sign-catalog` 不能再直接 push：个人账号的仓库不能把 GitHub Actions 加进例外（API 报 `must be part of the ruleset source or owner organization`）。它改成推 `catalog-signed` 分支、开 PR、手动启动 CI、自动合入。

## Consequences

- 所有会话都要排队等 macOS runner，合入比以前慢。`gh pr merge --auto` 让会话不必守着。
- 两个各自通过的 PR 合在一起仍可能变红。
- 仓库设置同时打开了 `allow_auto_merge` 和「Actions 可以创建 PR」。
- 工作流用 `GITHUB_TOKEN` 开的 PR 不会自动跑 CI，所以 `ci.yml` 加了 `workflow_dispatch`。

## Implementation Plan

受影响：`.github/workflows/ci.yml`、`.github/workflows/sign-catalog.yml`、`scripts/publish-catalog.sh`、`scripts/hooks/reference-transaction`、`CLAUDE.md`。CI 变红时修原因，不放宽规则。

## Verification

`gh api repos/majiayu000/mooring/rulesets --jq '.[].name'`，以及 `git push origin HEAD:main` 应报 `GH013`。

## Alternatives

- 分支保护只对非管理员生效：所有 Agent 都是管理员，等于没开。
- 自建 GitHub App 作为例外给 `sign-catalog`：要新增并保管一份凭据，目录又很少变，不值得。
- 只加本地 pre-push 钩子：其他克隆和 `--no-verify` 拦不住。
- 要求基于最新 main：会话多，会不停 rebase 和重跑 CI。

## More Information

重开条件：排队明显拖慢工作，或合并后变红仍频繁发生（后者考虑要求基于最新 main）。
