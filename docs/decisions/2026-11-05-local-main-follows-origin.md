# 本地 main 只跟随远端

状态：采用（2026-11-05，用户：「采用新规则」）

## 背景

多个会话把分支快进合并进本地 `main`，又都不推：上一个没推，下一个怕带上别人的提交，也不推。同时其他会话在 GitHub 上合 PR。到 11-05 本地领先 18、落后 128，同一个连接问题被本地和 PR #75 各修了一遍。

## 决定

本地 `main` 只用 `git pull --ff-only` 跟随 `origin/main`。改动在分支上做，rebase 到最新的 `origin/main` 后 `git push origin HEAD:main` 或开 PR。不要求等 CI。

`reference-transaction` 钩子（`scripts/hooks/`，`scripts/install-hooks.sh` 安装）拒绝把 `refs/heads/main` 移到 `origin/main` 之外。

## 否掉的方案

- 只写进文档：其他会话不一定读到，这次的分叉本来就是各自合理的做法累积出来的。
- `pre-commit` 和 `pre-merge-commit`：快进合并和 reset 不触发。
- 服务端保护分支：拦不住本地分叉，只会让推送失败得更晚。

## 代价

- 替代了之前「本地验证通过就直接合进本地 main」的做法。
- 钩子放在本机 `.git/hooks`，新克隆要跑一次安装脚本；全局 `core.hooksPath` 要有同名入口，安装脚本会补。
- 被拒的 `merge --no-ff` 会留下合并中的状态，要 `git merge --abort`，提示里写了。
