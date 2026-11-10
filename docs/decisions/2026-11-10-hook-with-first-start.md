---
status: accepted
date: 2026-11-10
decision-makers: 用户（选了「欢迎页默认勾选 + 状态栏提示 + 命令面板」，并问「是不是应该在设置里做这个」）、Claude
---

# Hook 不静默安装：欢迎页默认勾选随启动装，设置 › 通用是它的家，状态栏和 ⌘P 通过去

## Context

Hook 是 Claude Code 的状态和审批准不准的前提：不装就只能读终端画面来判断。11-08 做欢迎页时（`e4054c9`）否了自动安装，理由是它改的是别的工具的配置文件 `~/.claude/settings.json`，只留了页面下方一个「安装 Hook」按钮。

11-10 用户看欢迎页时问了三件事：要不要自动装、不装的人以后从哪装、是不是该放在设置里。查下来：

- 欢迎页的按钮不显眼，新用户多半直接点「启动」就走了，欢迎页之后不再出现。
- 之后只剩「设置 › 关于」里一行和 `fibo hooks install`，都要自己知道去找；状态不准时界面不会说原因是没装 Hook，更新 App 后 Hook 过期也只在「关于」里标红。
- 欢迎页的说明写「撤销：设置 › 关于」，那里其实只有安装按钮，没有卸载。

## Decision

1. **不静默装。** 窗口只在用户点了的时候写 CLI 的设置文件，这条不变。
2. **欢迎页：默认勾选「启动 Claude Code 时一起装上（推荐）」。** 勾着时，从欢迎页启动这个 CLI 会先写它的 Hook 再启动，这样第一段对话就走 Hook。取消勾选就不写。启动别的 CLI 不写 Claude Code 的设置；设置文件读不了时不写；写失败 Agent 照常启动，错误显示在顶部。原来的「安装 Hook」按钮去掉。
3. **设置 › 通用的「Agent Hook」卡片是唯一能装、重装、卸载的地方**（从「关于」挪过来，补了卸载），另有一个开关「没装 Hook 时提醒」，默认开，存 `<config>/hook-hint`。
4. **状态栏提醒。** 这台电脑上开着某个 CLI（有进行中的对话，或后台按进程认出来），它的 Hook 没装或过期，提醒又开着时，状态栏出现一项「Claude Code 没装 Hook」/「…的 Hook 过期了」，点开是设置 › 通用。欢迎页还在时不出现。
5. **⌘P** 搜 hook、钩子、状态不准，有一条「Agent Hook」，也通到设置 › 通用。

不做：启动时自动安装或自动更新过期的 Hook；在侧栏放提醒（侧栏不改版）；状态栏那一项点了直接安装（说明、卸载和关提醒都在设置里，入口只负责带过去）。

## Consequences

- 部分推翻 `e4054c9` 的 Rejected「自动装 Hook」：仍然不是自动，但默认选项从「不装」变成「装」，用户能在点启动前看到并取消。
- 更新 App 后 Hook 过期的老用户，开着 Claude Code 时会第一次在状态栏看到提醒；不想看可以在设置里关。
- 从没在这里跑过 Claude Code 的人（只用 Codex）不会被提醒。
- 窗口打开时、从窗口启动 Agent 时、设置里装卸之后各读一次 Hook 状态（后台起一次 `mooring hooks status`）。在终端里手动 `fibo hooks install` 之后，状态栏那一项要到下一次启动 Agent 或开关设置才消失。

## Implementation Plan

- `crates/mooring-app/src/workbench/hooks.rs`：窗口知道的 Hook 状态、`hint`、`installs_with_start`、状态栏那一项、欢迎页的启动。
- `crates/mooring-app/src/workbench/first_run.rs`：勾选框；`settings/hooks.rs`、`settings/general.rs`：卡片；`ui/hook_hint.rs`：提醒开关；`palette/`：`Hit::Hooks`；`frame_facts.rs`：`running`。
- 别在别处再加能写 CLI 设置的按钮；新入口都通到设置 › 通用。
- 别在 render 里为这个提醒扫全部会话，用 `FrameFacts::running`。

## Verification

- `cargo test -p mooring-app hooks first_run`
- `MOORING_FIRST_RUN_EVIDENCE=1`（`first_run_evidence.rs`）：全新 HOME 里勾着启动后 `fibo hooks status` 报已安装；加 `MOORING_FIRST_RUN_NO_HOOK=1` 后设置文件不存在。

## Alternatives

- 完全自动安装：见 Context，11-08 已否，理由仍成立。
- 维持按钮，只补后续入口：第一次就错过的人最多，后面全靠提醒补。
- 状态栏那一项点了直接装：少一步，但卸载、说明和关提醒要另找地方放。

## More Information

- 重开条件：其他 CLI 也有了 Hook 且写法不同；或状态栏提醒被普遍关掉。
- 来源：11-10 的会话（欢迎页 Agent 列表 #293 之后），提交见 `git log -- crates/mooring-app/src/workbench/hooks.rs`。
