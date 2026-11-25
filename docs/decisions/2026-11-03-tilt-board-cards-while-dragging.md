---
status: rejected
date: 2026-11-03
decision-makers: majiayu000
---

# 鸟瞰看板拖起卡片时倾斜

## Context and Problem Statement

鸟瞰看板的设计稿 `specs/overview-kanban/kanban.html`（老仓库 `tools/keepline`）里，拖起的卡片斜 2.2°、放大 3.5%。GPUI 只能旋转单色 SVG，普通 div 不能旋转，所以照做需要改 GPUI。

## Decision

不做。拖起的卡片保持水平，不旋转。

范围：只管鸟瞰看板的拖卡。窗格拖动的外观（悬停 ✥、原窗格变灰、胶囊卡片）不受影响。

## Consequences

* Good, because 不用长期维护一个改着色器的 GPUI 补丁，升级 GPUI 时少一处冲突。
* Bad, because 和设计稿有一处不一致；照设计稿核对时这一项要明确标「已否定」，不能悄悄跳过。

## Implementation Plan

* **Affected paths**: 无（main 上从未合入）。
* **Patterns to avoid**: 不要合入分支 `ui/board-drag-tilt`（`gpui-element-layer.patch`，加 `Window::with_element_layer`，改 Quad/Shadow/Sprite 和 `shaders.metal`）和 `ui/board-drag-approx`（1a06963，外框斜、内容正的近似版）。合 `bundle/*` 分支进 main 前去掉提交 66796e4，`bootstrap.sh` 里不能有 element-layer 补丁。

### Verification

- [ ] `grep -rn element-layer bootstrap.sh patches/` 没有输出
- [ ] `git branch --contains 66796e4` 不含 main

## Alternatives Considered

* 打 GPUI 补丁真旋转：效果对，但要维护改着色器的补丁。你在 2026-11-03 看过两种做法的对比后放弃。
* 不打补丁的近似版（外框、阴影斜，内容正）：看得出是假的，同时放弃。

## More Information

重开条件：GPUI 上游支持元素级变换。来源会话 16a9898b（2026-11-03）。

### 2026-11-18 复查

重开条件仍不满足。主线固定的 GPUI（`bootstrap.sh` 里 zed `2a857f5f`）只有 `MonochromeSprite` / `SubpixelSprite`（图标和字形）带 `TransformationMatrix`，`Quad`、`Shadow`、`PolychromeSprite` 没有，也没有元素级的变换或 `with_element_layer`；`patches/` 里没有 element-layer 补丁，66796e44 不在 main。主线已有的拖卡（`board_drawer.rs` 的 lift：整张卡跟手、阴影、松手飞回或飞到落点）保持水平，不放大，照本决定不动。`ui/board-drag-tilt`、`ui/board-drag-approx`、`bundle/sidebar-tilt*` 三条旧分支除 66796e44、1a069630 和合并提交外没有别的改动，侧栏部分早已在主线。
