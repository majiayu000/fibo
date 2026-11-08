---
status: accepted
date: 2026-11-08
decision-makers: majiayu000
---

# 官方中继的公网地址不进公开仓库，历史已重写

## Context

仓库是公开的。`d32157e`（11-06，docs(phone) 进度页）把官方中继的公网 IP 写进了 `docs/phone-app/progress.html`。用户：「中继服务器的地址，你帮我清洗掉吧，这个很难受啊，不能把这个遗漏了。」在「重写历史」和「给服务器换公网 IP」之间，用户选了重写历史。

## Decision

- 当前文件里的地址由 #226 换成说明和 `<中继地址>` 占位。
- 2026-11-08 用 `git filter-repo --replace-text` 只重写 `d32157e` 及其后代（204 个提交、77 个分支），其他提交的 SHA 和 GitHub 签名不变；临时关掉规则集 `main requires CI`，带租约原子强推，推完恢复。
- CI 的 `Workbench Rust checks` 第一步拒绝仍以 `d32157e` 为祖先的提交，防止旧分支把旧历史合回来。

不做：把地址换成 `relay.example.com`（读的人会当成真地址）；在仓库里写地址本身来做检查。

## Consequences

- 11-08 之前开的本地分支和 worktree 都建在旧历史上。直接 `git rebase origin/main` 会把两百个旧提交重放一遍，要用 `git rebase --onto origin/main <旧 main 上的分叉点> <分支>`。
- `refs/pull/*`（PR #142 起）和已有的 clone、fork 仍留着旧提交，要靠 GitHub Support 的「删除敏感数据」申请清理。清理后 compare 接口对 `d32157e` 返回 404，CI 检查自动放行。
- 本机分支 `feat/ios-acceptance`（`969eb60`）也写了这个地址，从没推过；推之前要先改掉。

## Verification

```sh
git log --remotes=origin -S'<地址>' --oneline   # 应为空
gh api repos/majiayu000/mooring/compare/d32157efe3f5acda9f5ede4b3a6c8b0df1753201...main --jq .status   # diverged 或 404
```

## Alternatives

- 给腾讯云服务器换公网 IP：最治本，泄露的旧地址失效；要重签证书、手机重配对。用户这次没选，仍可补做。

## More Information

演练发现 `filter-repo` 默认去掉 GPG 签名，GitHub 网页合并的提交会因此全部换 SHA；用 `--refs <分支> ^<祖先路径外的父提交>` 限定范围后只改必须改的提交。
