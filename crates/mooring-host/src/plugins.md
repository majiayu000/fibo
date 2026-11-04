# 改 Mooring：插件和外观

给 Coding Agent 看：用户让你给 Mooring 加个命令、换个左栏排法、在窗格上显示点什么、事件来了发通知、换完成动画或改状态颜色时，照这份做。全部是写文件，不用编译 Mooring。

## 在哪

配置目录是 `${XDG_CONFIG_HOME:-$HOME/.config}/mooring`，下面简称 `<配置>`。

- 插件：`<配置>/plugins/<id>/`，一个目录一个插件。
- 状态颜色：`<配置>/colors.json`。
- 快捷键、终端字体、完成动画、钉住提问这些在 App 的「设置」里改，别手改它们的文件。

## 规矩

- **一个想法一个插件**，新建目录，别把新东西塞进已有插件。
- 别改用户已有的插件，也别改 `plugins/disabled.json`（那是设置页的开关），用户明确要求时才动。
- 改完一定跑 `mooring plugins`，全是 ✓ 才算写好；有 ✗ 就按它说的改，再跑。
- 写好后让用户在 Mooring 的 设置 › 插件 点「重新读取」，马上生效；Agent 类要等 Mooring 后台下次启动。设置 › 插件能看到每个插件生效了没有，也能停用、卸载。
- 现成的插件不用自己写：`mooring plugins available` 列出 App 附带的（加 `--remote` 再查远程目录），`mooring plugins install <id>` 装上。
- 插件只能用下面列的这几类。做不到的（自己画界面、改会话状态、读对话原文）就直说做不到，别改 Mooring 的安装文件。

## 步骤

```sh
d="${XDG_CONFIG_HOME:-$HOME/.config}/mooring/plugins/git-tools"
mkdir -p "$d"
# 写 $d/plugin.json 和它提供的文件（见下）
mooring plugins            # 检查；有问题以 1 退出
```

## plugin.json

```json
{
  "api": 1,
  "id": "git-tools",
  "name": "Git 小工具",
  "version": "0.1.0",
  "description": "右键菜单里看 git 状态和改动",
  "provides": ["actions"]
}
```

- `api` 固定写 `1`。`id` 只用小写字母、数字和 `-`，不超过 40 个字，最好和目录名一样；和别的插件重名时后一个不加载。
- `name` 必填；`version`、`description` 可选；不认识的字段整份拒绝。
- `files` 可选：插件还要带上的其他文件（命令要跑的脚本），相对插件目录写。不写进来的文件从插件目录安装时不会被带上。
- `touches` 可选：setup 会改的别的程序的文件，如 `["~/.claude/settings.json"]`；`network` 可选：命令会不会联网。安装确认里照写给用户看，Mooring 不拦截，所以要如实写。
- 命令行安装会跑命令的插件时先列出这些，交互终端里问 y/N，非交互（Agent、脚本）要加 `--yes`。
- `mooring` 可选：插件需要的插件级别（现在是 3：`setup`、`quota`、`files` 是 2 级加的，`themes` 是 3 级加的）。不写时按用到的类型和字段自动算，一般不用写。级别不够的 Mooring 会拒绝它并提示更新。
- 新版本 Mooring 才有的类型和字段，旧版本会跳过并在设置页写一条警告，其余照常加载；只有一类都不认识时才整个不加载。
- `provides` 至少一种：`actions` `sidebar` `decorations` `hooks` `rules` `effects` `setup` `quota` `themes` `agent`。写了哪种就必须有那种的文件；有文件没写进 `provides` 的不会加载。
- 每个类型文件里也都写 `"api": 1`（`agent.json` 除外，见最后），不认识的字段同样整份拒绝。

## 所有插件命令都拿得到的环境变量

hooks、decorations、quota 的 command、setup/teardown 都会带上（actions 在终端里跑，用 `{cwd}` 这类模板）：

- `MOORING_PLUGIN_ID`、`MOORING_PLUGIN_DIR`：哪个插件、它的目录（找 `files` 里带的脚本）。
- `MOORING_PLUGIN_DATA`：插件自己的数据目录，更新时保留，卸载时删除。
- `MOORING_BIN`、`MOORING_RUNTIME`：`mooring` 命令和它连的后台，插件里 `"$MOORING_BIN" list` 和在 Mooring 终端里一样能用。
- `MOORING_OPTION_<KEY>`：插件的每个设置项（见下）。开关是 `1` / `0`。

## 设置项（`plugin.json` 的 `options`）

```json
{
  "api": 1, "id": "notify-me", "name": "通知我", "provides": ["hooks"],
  "options": [
    {"key": "only_errors", "title": "只在出错时通知", "type": "boolean", "default": false, "description": "打开后只在出错时弹"},
    {"key": "interval", "title": "间隔（秒）", "type": "number", "default": 300},
    {"key": "sound", "title": "声音", "type": "choice", "choices": ["Glass", "Ping"], "default": "Glass"}
  ]
}
```

- `type`：`string`、`number`、`boolean`、`choice`（要写 `choices`）。`key` 只用小写字母、数字和 `_`，最多 20 个。
- 用户在 设置 › 插件 里每个插件下面改，或者 `mooring plugins set <id> <key> <值>`；值存在数据目录的 `options.json`，下次运行就用。
- 值只以环境变量传给命令，不会拼进命令文字里：脚本里读 `"$MOORING_OPTION_SOUND"`，不要让值变成命令的一部分。
- 凡是用户可能想要不同值的参数都做成设置项，不要写死在脚本里。

## 运行记录

`mooring plugins log <id>` 列出最近 50 次：setup、teardown 和 hook 每次都记，装饰和 quota 命令只在结果变化时记（第一次出错、恢复）。最近一次出错也显示在 设置 › 插件 那个插件的说明里。

## actions：右键菜单和 ⌘P 里的命令（`actions.json`）

```json
{
  "api": 1,
  "actions": [
    {"id": "git-diff", "title": "看这里的改动", "command": "git -C {cwd} diff --stat && git -C {cwd} diff", "run": "split"},
    {"id": "resume-tab", "title": "在新标签里接着这个 Claude 会话", "command": "claude --resume {session_id}", "run": "tab", "when": "agent"}
  ]
}
```

- `command` 里能用 `{cwd}` `{session_id}` `{transcript}` `{task}` `{agent}`，代入时会自动加引号，别自己再包引号。
- `run`：`split`（默认，在旁边开一块分屏跑）、`tab`（新标签）、`paste`（打进当前终端但不回车，让用户看了再发）。
- `when`：`always`（默认）、`agent`（窗格里有 Agent 会话）、`shell`（只是 shell）、`task`（标签挂着待办）。

## sidebar：左栏的排法（`sidebar.json`）

```json
{
  "api": 1,
  "presets": [
    {"id": "focus", "name": "专注", "group": "status", "fold": ["running", "done", "stopped", "fresh", "stale"], "detail": "always", "limit": 8}
  ]
}
```

- `group`：`flat`（一列，需要你的在前）、`status`（按状态分段）、`directory`（需要你的置顶，其余按目录分）。
- `fold`：哪些段收成一行，点开才展开。可选 `running` `done`（等你验收）`stopped` `fresh` `stale`；`attention`（需要你）不能收。
- `detail`：会话行什么时候显示第二行：`never`（默认）、`attention`（需要你的和选中的）、`always`。
- `limit`：左栏最多列几行，剩下的进鸟瞰，默认 12。
- `machines`：另外几台电脑的会话放哪，只有 `"group": "recent"` 能写：`mixed`（和本机混在一列，内置「会话优先」）、`rail`（一条竖栏选一台，内置「机器栏」）、`sections`（每台一个可折叠的区，内置「按机器」）；不写是 `below`（接在本机列表后面）。竖栏和断开机器的灰色行由 Mooring 画，插件改不了。
- 新排法出现在左栏底部的「选择视图」和命令面板里，和内置的并列，不会替换它们。`rail`、`sections` 只在配了其他电脑时出现。
- 还有 `"group": "wasm"` 加 `"wasm": "<文件>.wasm"`，用沙盒里的 WebAssembly 自由排；要用 Rust 编译，没被要求就别用。

## decorations：窗格上显示一小段命令输出（`decorations.json`）

```json
{
  "api": 1,
  "decorations": [
    {"id": "node", "command": "[ -f package.json ] && node -v", "place": "title", "refresh_s": 60},
    {"id": "last-commit", "command": "git log -1 --format='最近提交 %cr：%s' 2>/dev/null", "place": "tip", "refresh_s": 30}
  ]
}
```

- `command` 用 `sh -c` 在窗格目录里跑，显示输出的第一行；退出码不是 0 或没输出就什么都不显示，所以不适用的目录让它安静失败。
- 能读到 `MOORING_CWD` `MOORING_AGENT` `MOORING_SESSION`，以及 shell 上一条命令的 `MOORING_LAST_EXIT` `MOORING_LAST_DURATION_MS`（还没有时为空）。
- `place`：`title`（默认，窗格标题栏）或 `tip`（悬停标题时的卡片里单独一行）。
- `tone`（只对标题栏）：`muted`（默认）、`ok`、`warn`、`error`，跟着状态颜色。
- `when`：`always`（默认）、`agent`、`shell`。`refresh_s` 多久重跑一次，默认 10；`timeout_ms` 默认 1000。命令要快，别在这里联网。

## hooks：会话变化或命令跑完时执行（`hooks.json`）

```json
{
  "api": 1,
  "hooks": [
    {"id": "notify", "on": ["waiting", "error", "done"], "command": "osascript -e \"display notification \\\"$MOORING_DETAIL\\\" with title \\\"$MOORING_TITLE\\\"\"", "timeout_ms": 5000}
  ]
}
```

- `on`：`started` `waiting` `error` `done`（会话），`command_done` `command_failed`（shell 里敲的命令跑完）。
- `command`（`sh -c`）和 `webhook`（POST 一个 JSON）二选一或都写。事件以 JSON 从标准输入给，也有环境变量：`MOORING_EVENT` `MOORING_SESSION` `MOORING_AGENT` `MOORING_TITLE` `MOORING_CWD` `MOORING_SPACE` `MOORING_DETAIL` `MOORING_TERMINAL`，命令事件还有 `MOORING_EXIT_CODE` `MOORING_DURATION_MS`。
- `timeout_ms` 默认 5000，最多 30000；`min_ms` 只对命令事件，跑得比它快的不报。
- hook 只能知道，不能改会话；拿不到对话原文，也拿不到命令行本身。Mooring 没开时后台也会跑 hook。

## rules：哪些命令算跑过验证（`rules.json`）

```json
{"api": 1, "verify": [["just", "test"], ["./scripts/ci.sh"], ["task", "*"]]}
```

每条是命令开头的几个词，按最后一段路径比（`./node_modules/.bin/vitest` 算 `vitest`），`*` 匹配任意一个词。Agent 跑了这些命令，退出码和测试数会算作验证证据。只能加，不能去掉内置的。

## effects：标记完成时的动画（`effects.json`）

```json
{"api": 1, "complete": {"style": "stamp", "duration_ms": 2400, "land": "sidebar"}}
```

- `style`：`seal`（印章，默认）、`stamp`（落印）、`sweep`（扫光合上）、`dissolve`（化进侧栏）、`fade`（只淡出）。
- `duration_ms` 在 600–4000 之间；`land`：`sidebar`（落到侧栏那一行）或 `none`。
- 每个插件的设定是 设置 › 外观 › 标记完成动画 里多出来的一个选项（用插件的 name 当名字），装上不会自动换掉用户选的；和内置某种一模一样的（同 style、默认时长和落点）不另列。系统开了「减弱动态效果」时一律只淡出。

## setup：装上时改别的程序，卸载时改回来（`setup.json`）

```json
{"api": 1, "setup": "sh setup.sh", "teardown": "sh teardown.sh", "timeout_ms": 10000}
```

- `setup` 在装上、更新、打开时运行，`teardown` 在停用、卸载时运行；都在插件目录里用 `sh -c` 跑。`MOORING_PLUGIN_DIR` 是插件目录，`MOORING_PLUGIN_DATA` 是插件自己的数据目录（`<配置>/plugin-data/<id>`，更新和重装都保留）。
- 两条都要写：装上时改了什么，`teardown` 就要能原样改回去。原来的设置存进 `MOORING_PLUGIN_DATA`。
- `setup` 会在已经装过的情况下再跑（更新时），要能重复运行。
- 失败（非 0 退出或超时，`timeout_ms` 100–30000，默认 10000）时，打印的内容会显示在设置页；`teardown` 失败时插件不会被卸载或停用。
- 卸载成功后 `MOORING_PLUGIN_DATA` 整个删除；`teardown` 要用的东西（原来的设置）只在卸载前读。
- 例子：`plugins/claude-quota`（接上 Claude Code 的状态栏，原来的状态栏照常显示）。

## themes：主题（`themes/*.json`）

只有数据，不运行任何东西。每个文件就是一套主题，格式见 `mooring --skill` 的「主题」。装上后出现在 设置 › 外观，停用或卸载就消失；正在用它时回到默认主题。用户点「调整」时存成自己的副本，不写回插件。背景图的相对路径从插件目录算，图片要写进 `plugin.json` 的 `files`。例子：`plugins/theme-tide`。

## quota：某个 Agent 的额度（`quota.json`）

```json
{"api": 1, "sources": [{
  "agent": "claude",
  "file": "statusline.json",
  "windows": [
    {"minutes": 300, "used": "/rate_limits/five_hour/used_percentage", "resets_at": "/rate_limits/five_hour/resets_at"},
    {"minutes": 10080, "used": "/rate_limits/seven_day/used_percentage", "resets_at": "/rate_limits/seven_day/resets_at"}
  ]
}]}
```

- `agent`：CLI 的可执行文件名或名字。显示在状态栏的额度里，和内置的读数比谁新。
- 数据来源二选一：`file` 是一个 JSON 文件（相对 `MOORING_PLUGIN_DATA`，也可以写绝对路径或 `~/`），文件变了就重读；`command` 是输出 JSON 的命令，App 开着时每 `refresh_s` 秒（60–86400，默认 300）在插件目录里跑一次，最多 20 秒。
- `windows` 里每个窗口写 `minutes`（300 显示成 5h，10080 显示成 周），`used` 和 `resets_at` 是 JSON 指针（以 `/` 开头）：已用百分比、重置时间（Unix 秒）。取不到 `used` 的窗口不显示；一个都没有就不显示这家。
- `at`、`plan` 可选，也是 JSON 指针：记录时间（Unix 秒，不写就用文件修改时间或命令运行时间）、套餐名。
- 要联网查的话写在 `command` 里；Mooring 自己不联网，这类插件的说明里要写清楚。

## agent：让 Mooring 认识一个新 CLI（`agent.json` 或 `agents/*.json`）

这类最复杂：声明 CLI 的可执行文件、启动和恢复命令，以及怎么从它的会话记录（JSONL 或 SQLite）里读出回合状态。用户要接一个新 CLI 时才写。先找一个现成的例子照着改（Mooring 源码里的 `plugins/opencode/agent.json`），写完同样用 `mooring plugins` 检查。

## 状态颜色（`<配置>/colors.json`）

```json
{
  "status": {"working": "#e8c547", "done": "#4cc38a"},
  "seen": {"title": "#b9b7b4", "status": "#7d7c7a", "dot": "ring"}
}
```

`status` 的键：`waiting` `error` `review` `paused` `running` `stale` `working` `done` `ready`；`seen` 是看过的会话行，`dot` 取 `ring` `fill` `fade`。缺的、读不了的保持默认。改了要重开 Mooring 才生效。

## 例子

用户说「右键菜单里加一个用 VS Code 打开这个目录」：

```sh
d="${XDG_CONFIG_HOME:-$HOME/.config}/mooring/plugins/open-in-vscode"
mkdir -p "$d"
cat > "$d/plugin.json" <<'JSON'
{"api": 1, "id": "open-in-vscode", "name": "用 VS Code 打开", "provides": ["actions"]}
JSON
cat > "$d/actions.json" <<'JSON'
{"api": 1, "actions": [{"id": "code", "title": "用 VS Code 打开这个目录", "command": "code {cwd}", "run": "paste"}]}
JSON
mooring plugins
```

全是 ✓ 后告诉用户：重启 Mooring，在窗格上右键或 ⌘P 搜「VS Code」就能看到。
