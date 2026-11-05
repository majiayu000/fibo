# Codex 启动图标验证

关联 [issue #94](https://github.com/majiayu000/mooring/issues/94)。决定采用已有的进程身份显示图标，聊天关联和工作状态仍按各自证据判断。

## 同场景原生截图

使用仓库 `visual-check.py shots` 对修复提交及其父提交分别构建，再用同一份视觉夹具生成 `UnboundAgent@1225`、`Main@1225`。检查在独立 worktree 和独立夹具状态文件中运行。测试中的 Codex 是进程名可被正常识别的 `/bin/sh` 替身，不调用模型、不创建聊天记录；截图来自真实 GPUI 窗口渲染。

| 修复前 | 修复后 |
|---|---|
| [原图](before.png)：选中标签没有图标，窗格显示普通终端图标和「终端」 | [原图](after.png)：标签和窗格均有 Codex 图标，窗格名称为 Codex |

复现截图场景：

```sh
MOORING_FIXTURE_UNBOUND_AGENT=1 python3 scripts/visual-check.py shots <fix-commit> <out> --pages UnboundAgent@1225,Main@1225
```

## 进程生命周期

在独立 host 的真实交互式 zsh 中通过 `SendText` 提交命令，走原有的回车后识别链路，不手动请求扫描、不伪造会话：

| 过程 | 后台报告的 Agent | 会话数 |
|---|---|---|
| 启动 Codex 替身，尚无聊天记录 | Codex，无额外提示文字 | 0 |
| Codex 退出，回到 shell | 无 | 0 |
| 同一终端启动 Claude 替身 | Claude | 0 |
| Claude 退出，回到 shell | 无 | 0 |

已有会话的 `Main` 场景也作了截图检查。完成会话原有的历史展示保留；新的未关联 Agent 优先于旧会话的图标。

## 检查与边界

- host 单元测试、app 测试、core 的事实来源与文件大小门禁、相关包 clippy、格式检查、visual-test 构建通过。
- 两个回归用例分别在移除后台身份写入、移除界面进程身份取值后失败，且撤回版本均能编译。host guard 限定 `--lib`，避免把其他集成测试筛选出的零用例误判为回归用例未运行。
- release 窗口性能门禁通过；终端帧 6.30 M 指令，整窗重画 73.48 M，低于各自基线；重画逐像素检查一致，基线未上调。
- 尚未替换用户正在使用的 App，未验证真实 Codex 0.166.0 的完整聊天、登录及恢复流程。GitHub CI 的结果以 PR 为准。
