---
status: accepted
date: 2026-11-09
decision-makers: 用户, Claude
---

# 手机看得到哪些空间被「先不管」，能点「接着管」

## Context

11-07（55fb6d9）定的是：先不管的空间，它的会话和空间本身都不发给手机和别的电脑，手机连「有这么个空间」都不知道。

11-09 用户在电脑上把 vsr、her、youtube 点成先不管，手机上等你验收从 17 个掉到 5 个。用户问「为什么打开之后只有 6 个等我验收」「我如何在那边决定空间的概念呢」。手机既看不出少了什么，也没法改回来。

## Decision

- `list` / `cards` 多一个 `quiet`：先不管的空间，只有 id、名字、目录，**不带它们的会话**。
- 新 op `spacequiet`（完全控制才能用）：和电脑上的开关是同一个 `Edit::SetSpaceQuiet`。
- 手机主页最下面折一行「先不管的空间 · N」，展开后每个空间旁边有「接着管」。
- 主页标题下面放回空间筛选（11-07 挪进了搜索页）。

## Consequences

- 先不管的空间里的会话仍然不到手机上，55fb6d9 的约定只放宽了「空间名字」这一项。
- 旧电脑不发 `quiet`、不认 `spacequiet`：手机上不出这一行。旧手机忽略 `quiet`。
- 手机上没有「先不管」这个动作，只有「接着管」。

## Alternatives

- 先不管的空间的会话也发给手机、灰着显示：否定，和电脑端「放一边就别打扰」的意思相反。
- 手机上也能点「先不管」：用户没要，先不做。

## More Information

- 来源：用户 11-09 原话；Supersedes（部分）：55fb6d9。
- 守卫：`remote::list::tests::a_space_put_aside_is_not_listed_to_other_devices`、`remote::cards::tests::a_space_put_aside_or_taken_back_says_the_spaces_put_aside`、`mobile/test/spaces.test.ts`。
