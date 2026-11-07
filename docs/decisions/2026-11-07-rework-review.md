---
status: accepted
date: 2026-11-07
decision-makers: majiayu000
---

# 试两周按提交复盘返工

## Context

每次提交的 `Rejected:` 和 `Guarded-by:` 记得很细，但没有东西回答「同一类错犯了几次」。yetone/magpie 每晚复盘当天提交，记成规则、证据和出现天数，满 3 天进规范。用户看过方案后说「可以 都做吧」。

第一遍（11-01 到 11-07，536 个提交）找出了重复的类，其中一类出现 4 天，所以这个方法对我们有用。

## Decision

`docs/lessons.md` 记重复犯的错，`scripts/rework-pairs.py` 提供配对。满 3 天的规则升进 `CLAUDE.md`。能写成测试的先写测试。

不做：每晚定时自动跑；给单次的错也写一条；抄 magpie 的规则原文（要有这个仓库的提交做证据）。

## Consequences

- 复盘要读提交的 `Why:`，有成本。配对脚本只给输入，归类仍要人看。
- 91 对里有些是看了效果接着调，不是错。比例不能当返工率的硬指标。

## Implementation Plan

受影响：`docs/lessons.md`、`scripts/rework-pairs.py`、`CLAUDE.md`。

## Verification

`python3 scripts/rework-pairs.py --since 2026-11-01` 能列出配对；`CLAUDE.md` 里有 `docs/lessons.md`。

## Alternatives

- 每晚定时跑：还不知道一周一次够不够，先手动。
- 只靠提交说明：它回答不了跨提交的问题。

## More Information

**放弃条件：** 11-21 再跑一遍。如果没有规则在两周内再次被用上或挡住错误，或者同类错照犯（新出现的天数没有变少），就删掉 `docs/lessons.md` 和脚本，这份改成 rejected。
