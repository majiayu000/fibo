---
status: accepted
date: 2026-11-11
decision-makers: 用户确认改成下拉选择
---

# 默认 Agent 用下拉选择

## Context

用户截图中七个 Agent 的分段按钮不能收缩，把左侧说明挤成一字一行。
`6e54636` 已将按钮放到说明下方换行；用户随后确认使用下拉选择。

## Decision

采用只显示当前 Agent 的下拉选择。候选项仍来自 `Agent::all()`，通过既有
`SettingsAction::Change` 保存；打开、取消和键盘高亮只存在于设置视图内。

## Consequences / Implementation Plan

修改 `crates/mooring-app/src/workbench/settings/general.rs`、`settings.rs` 和
`settings/panel.rs`。点击选项或回车保存，Esc、再次点击按钮或点击外部关闭菜单。
旧的设置值和保存格式保持原样；其他设置行不改。

## Verification

`harness/agent_picker_evidence.rs`：在独立的 `XDG_CONFIG_HOME` 和 0700 runtime 中，
用 `MOORING_AGENT_PICKER_EVIDENCE=select` 启动 visual-test 构建，执行真实鼠标、
键盘事件并读回宿主；同一 runtime 重启后用 `readback` 检查保存结果。
`visual-check.py shots` 对比主线已有换行布局和本次下拉布局（Settings-General）。

## Alternatives / More Information

不继续用多行分段按钮：Agent 名称多且长，常态下展示当前选择就足够。
不另建通用选择框框架。只有其他设置实际需要时才考虑复用。
验收若发现文字仍挤压、候选项无法选择或保存失败，就不交付。

## 2026-11-11 审查补充：选中项必须可见

注册 Agent 多于菜单容纳的行数时，打开菜单和方向键移动都要把选中项滚到可见范围。
沿用 GPUI ScrollHandle；不另算一份滚动偏移。新菜单在首次布局拿到视口后补一次定位，
避免 ScrollHandle 在视口尚未建立时丢掉打开请求。
`scripts/agent-picker-scenes.py BIN_DIR OUT` 使用 12 项和 20 项的隔离注册表，
实际检查行的绘制边界、首尾循环、重新打开与宿主重启后的选中项；纳入必需的 Rust CI。
