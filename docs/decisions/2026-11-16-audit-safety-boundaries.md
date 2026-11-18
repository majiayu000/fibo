---
status: accepted
date: 2026-11-16
decision-makers: [user]
---

# 审计修复沿现有边界保护文件、Git 查看和手机配对

## Context

11-16 的 `origin/main@2efacb85` 审计指出：配置读坏后保存会覆盖原文件，OSC 8 的 file 链接绕过普通路径的可执行文件保护，插件的 git status 没有原生 diff 的基础保护，新配对二维码长期有效。用户要求逐项优化，明确 `fibo` 命令继续不设防。

## Decision

- 配置的保存入口严格读取原文件；不存在可以新建，不能读取或理解则拒绝保存，并沿现有设置错误区显示原因。快捷键的未知动作不生效但按原 JSON 保留，修改已知动作和恢复默认都不删它。
- 本机 `file://` 链接解码为路径后走 `open_path::plan`，脚本等可执行目标只在访达显示；网页链接和本机文件链接的悬停说明都带实际 URI。远程窗格沿现有规则拒绝打开本机同名文件。
- 原生 Git 查看和插件命令共用 command-scope 的 `core.fsmonitor=false`、`safe.bareRepository=explicit`、`core.quotepath=false` 与禁用可选锁。覆盖继承的 `GIT_CONFIG_COUNT`，移除 `GIT_CONFIG_PARAMETERS`，避免继承配置重新打开 fsmonitor。实测同长度文件修改时 `git status` 和 `git ls-files -m` 仍会执行仓库的 clean filter，因此默认 `pane-facts` 改为只读索引的“已暂存 N”，不再自动统计未暂存改动。这是查看命令的基础保护，不是对可执行插件或完整 Git 行为的沙箱。
- 新配对条目带 `pairing_expires=创建时间+600秒`，首个成功解密的手机消息在转发前清除此期限。只有握手回复、未能解密、过期或保存失败都不能把未使用链接变成永久凭据。
- 已有无期限字段的设备继续有效；旧版创建但从未使用的链接不追溯过期，无法从会滚动的日志可靠区分它与已配对手机。不做日志回填，也不改变已配对手机协议。
- 手机权限表列明当前只读操作，未知操作默认拒绝。插件下载允许最初的本地 `file:` 来源，但服务器重定向只允许 HTTPS。

## Consequences

坏文件需用户修复后才能从设置页保存；其原始字节保留。新二维码需在 10 分钟内首次使用。旧二维码如需撤销须取消该设备并重新配对。完全控制的手机、已授权插件及本用户进程仍有其原有能力。

## Implementation Plan

保存边界在 `keymap.rs`、`ui/space_themes.rs`、`ui/terminal_text.rs` 和 `devices.rs`；Git 入口在 core 的 `git.rs`；首次配对确认在 relay 解密与转发之间。不得从 `last_seen` 日志推断身份或为本地 `fibo` 命令增加权限层。

## Verification

- `cargo test -p fibo-app saving_`：坏 JSON 与语义坏值保存后逐字节不变，未知快捷键恢复默认后仍在。
- `cargo test -p fibo-app encoded_file_links`：URL 编码的 `.command` 目标得到 Reveal 计划。
- `cargo test -p fibo-core native_and_plugin_inspection`：夹具的 fsmonitor 未保护时写出标记，原生及插件调用保护后不产生标记；clean-filter 夹具证明旧 `status` 会执行，实际默认插件新命令只显示已暂存文件数且不执行。
- `cargo test -p fibo-host devices::tests`：损坏的撤销/退休列表不被覆盖，过期链接不列入路由、不被确认，确认后和旧设备不再过期。
- `cargo test -p fibo-remote roles_allow_only_what_they_name`：已知只读与写操作权限保持，未知操作均拒绝。

UI 截图、真机配对及现有旧二维码的现场行为需独立验收；单测不是这些验收的替代。

### 2026-11-16 本地结果

上述 focused 单测及相邻的 keymap、terminal_text、space_themes、open_path、screen 用例实际运行通过；guard、file_size 与 i18n 通过，未更新基线。两条 relay 回归通过真实本地 WebSocket/Noise 验证重放握手与认证首帧的差别。初次 i18n 扫描把 `cfg(all(test, unix))` 中的测试断言当产品文案，改成它认识的 `cfg(test)` 模块边界后通过。

尚未安装新插件或替换 App；已有 pane-facts 0.1.1 仍须更新才能去掉自动 worktree 检查。真手机和 UI 截图未验，Windows/Linux 没有实际运行。

## Alternatives

未采用一次性票据换长期密钥、日志回填、全新权限系统或插件沙箱：均超过这批小修范围。未禁用全部插件 Git 行为：插件目前就是用户授权的可执行命令。

## More Information

来源：`~/Documents/claude-pages/mooring/2026-11-16-repo-audit/index.html` A2、S1–S3、S6–S7；用户本次“fibo 不设防”的明确决定。若要追溯撤销旧未使用配对码，需要先确定不误伤既有手机的用户操作流程。
