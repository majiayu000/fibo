---
status: accepted
date: 2026-11-16
decision-makers: 用户（“都要做”）
---

# 在现有平台实现上补齐交付，各平台分别验收

## Context

用户要求设计并实现尚未收尾的工作，随后明确 Windows/Linux 正式版及 iOS 完整交付也要做。已有 Windows 安装器、Linux host 和 GPUI 后端、Expo iOS 模拟器构建；这些不等于各平台已经交付。

## Decision

复用现有实现。Windows x64 用 MSVC 和 Inno Setup；Linux 先支持 Ubuntu 24.04 x64/ARM64，以 Debian 包交付；iOS 用现有 Expo 配置和 EAS 的内部分发/商店构建。iOS 无签名设备 archive 用来验证设备编译，不能当成可安装 IPA。

签名身份从真实账号和证书获得，缺失时明确待验证；不生成替代身份，不把 unsigned、模拟器、容器、CI、发布和真人验收混成同一个“已完成”。本次不新增自动更新器或跨平台状态层。

## Consequences

Windows 继续由安装器更新；现有 ConPTY 没有无损交接，用户要先结束终端。Linux 通过包管理器安装和更新，图形会话、Secret Service 和通知仍需桌面验收。iOS 签名和真机需要设备及账号，编译成功不能解除这些条件。

## Implementation Plan

- `packaging/linux/package.py` 与独立工作流生成安装包，保留来源和哈希，实际安装后启动 host。
- `package-windows.yml` 在产品源码变化时打包，并走安装、已安装 host、卸载；保留签名状态和哈希。
- `mobile/eas.json` 定义真机内部分发与商店构建；设备 archive 和现有模拟器检查互补。
- 收尾范围及每轮回执归入 `docs/release/closeout-design.md`，不改写旧验收历史。

## Verification

`actionlint` 检查三个工作流；原生 runner 构建/安装；`terminal_chain` 验证真实系统终端；iOS 既编模拟器也编设备。每个平台最后都要实际打开、使用并记录未验项。

## Alternatives

否定另造跨平台发布框架、统一自动更新器和新凭据存储：现有工具足够支持当前交付。其他 Linux 发行版等出现具体用户与环境后再扩展，不由一个 Ubuntu 包推断支持。

## More Information

见 `docs/platform/index.html`、`docs/release/acceptance.md` 和 `docs/phone-app/walkthrough.md`。平台构建或真实使用证伪上述选择时再修改决定；没有证书不是降低验收门槛的理由。
