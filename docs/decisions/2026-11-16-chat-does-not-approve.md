---
status: accepted
date: 2026-11-16
decision-makers: majiayu000
---

# 聊天消息不能回答审批

## Context

用户要求落实 11-16 审查的六条待办。审批菜单默认停在允许项，手机发送文字后跟随的回车会选择它；离线补发、图片路径、排队/立刻发送走的也是终端输入。“拒绝并告诉它”原先固定等 600 ms，#330 的拒绝又可能被互斥挡住。

要支撑的决定：这几条聊天入口能否安全使用。验收是审批未被回答、命令未执行；不能只看手机显示发送失败。识别不出真实 CLI 菜单或真机行为不符时，保持未验，不能据假终端通过发布。

## Decision

采用：在 host 的聊天输入边界，用写 PTY 的同一把锁读取当前审批/信任菜单。文字与图片路径写入前检查一次；处理粘贴后的提交键再次检查，覆盖两次写之间出现审批的竞态。各 CLI 的 queue/steer 按键由已有 Adapter 提供，经 `SendTurn` 一次请求处理，手机不拼接裸按键。

手机在已知审批期间保留输入和离线队列。拒绝必须得到成功结果，刷新确认原对话的审批退出后才发说明；失败、超时或换了对话不发送，说明留在输入弹窗。允许/拒绝入口共用已有 #330 所用的互斥思路。

## Consequences

远程协议升为 2：旧手机连新电脑收到“请把两端更新到同一版本”；新手机连旧电脑检查 hello 的版本并停止重试。普通 `send` 即使缺少可选的会话字段，也在 host 写入处受到保护。显式终端按键仍可回答菜单；它是用户在“看终端”里的操作，不能当成聊天提交的实现。新 `SendTurn` 要求配套 host；混用旧 host 不回退到不受保护的裸按键。原生 hello 的 Build 明确声明 `guarded_chat`，远程入口缺少它就拒绝，覆盖“CLI 换了、后台还在跑旧代码”的情况；磁盘数据格式不变。

弱网采用已有 #370 的按实际传输进度续期，并补齐其缺失的电脑端上传 ACK：只有字节写入会话后才确认，ACK 和回复在同一把加密锁下入队，保持 Noise 序号。完整消息没到之前也能看到进度，其他终端事件和后续请求的 ACK 不会无限延长停住的请求。

## Implementation Plan

- `host/client/typing.rs`：文字、图片路径和最终提交键的检查；审批回答实现因文件大小限制迁到同文件。
- `remote/turn_input.rs`：把排队、立刻发送交给 host。
- `mobile/src/actions.ts`、`app/approve.tsx`、`app/chat.tsx`、`remote/pump.tsx`：拒绝回执、原对话身份、草稿与待发保留。
- 完整真机走查新增审批期间发送和面容锁用例。

## Verification

`cargo test -p fibo-host --test remote_message_safety` 检查真实 PTY 收到的字节：Claude/Codex 审批期间各入口均为空；显式拒绝后只有 Esc、说明和回车。`approval_opened_by_paste_blocks_the_later_enter` 对撤掉第二次检查的版本必须失败。

`cd mobile && bun test test/approval.test.ts` 检查允许占用、拒绝报错、等待菜单关闭和会话被替换四条路。真手机和真 CLI 仍须按 walkthrough 全表验收。

## Alternatives

否定只禁用手机按钮：旧 APK 和重连补发仍会漏。否定固定延时：机器和网络负载不能用 600 ms 表示。否定出错后自动重发：提交结果可能未知，不能再制造一次输入。

## More Information

待办：`d4e7f805-815f-42d0-a7f8-c73283a72f8b`、`9722465c-206a-40b2-953f-b90969016e8b`。重开条件：新的 Agent 审批形态无法通过现有屏幕/API 识别。
