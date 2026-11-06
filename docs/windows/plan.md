# Windows 原生支持：现状和下一步

要支撑的决定：`mooring-host` 的 Windows 工作有多大、按什么顺序做。放弃条件见「第 2 步」。

结论：`mooring-core` 已能编译到 Windows。host 不是小 `cfg` 能解决的，要新设计三处：终端、连接、交接。最该先做的是用真 ConPTY 验证读屏，它决定整个方案成不成立。

下文的「未运行」指只编译过，没有在 Windows 上跑过。

## 现在能编译什么

用 `x86_64-pc-windows-gnu` 跑 `cargo check`（2026-11-07，基于 `a405a5f`）：

| crate | 结果 |
|---|---|
| `mooring-core`（含测试目标） | 通过，clippy 无警告；未运行 |
| `mooring-relay`、`mooring-remote`、`mooring-plugin-host` | 不用改就通过；未运行 |
| `mooring-host`（lib） | 21 个文件 249 个错误 |
| `mooring-app` | 没试。`.deps/zed` 里有 `gpui_windows`，但 `patches/gpui-*` 和窗口代码没看 |

`libghostty-vt-sys` 的构建脚本用 Zig 交叉编出了 `ghostty-vt.dll` 和 `.lib`，读屏用的解析库在 Windows 上至少能编出来，同样未运行。

core 里的 Windows 实现，全部未运行：

- 进程组用 job object（`process.rs`）。std 不能让进程一启动就在 job 里，它刚启动时开的子进程会漏在外面。
- 进程是否存在用 `OpenProcess`，无权打开算存在，避免误删它还在用的文件。
- 本地时间用 CRT 的 `localtime_s` 和 `gmtime_s`。CRT 优先看 `TZ`，从 Git Bash 启动时要留意。
- 文件锁改用 std 的 `File::lock`。macOS 上它就是原来的 `flock`。

core 里故意报错、还不能用的：

- `delivery::attempt` 回「Windows 版后台还没有做好」。
- `cli_entry` 放不了 PATH 链接。
- 插件命令要 `sh`，只有 Git for Windows 且在 PATH 上才有，否则启动失败并报错。

core 编译得过但在 Windows 上还不对的，第 0 步处理：

- 读 `HOME` 的有 8 处，Windows 没有它（Git Bash 下才有，且路径写法不同）。没有时配置目录会找不到，插件功能报错。
- `secret.rs` 写死 `/usr/bin/security`：存时报错，读时当没有。要换成凭据管理器。
- `create_private_dir` 只继承上级权限，在用户目录外不算私有。

## mooring-host 清单

错误数来自编译：`ring` 和 SQLite 的 C 代码用 scratch 里一个调 `zig cc` 的小脚本编过，没有安装任何东西。这是下限：未解析的类型会盖住后面的错误，`main.rs` 和测试都还没编到。「要做什么」来自读代码，ConPTY 的行为大多凭记忆，要第 2 步核实。

| 方面 | 文件（错误数） | 合计 | Windows 上要做什么 | 大小 |
|---|---|---:|---|---|
| 终端 | `pty.rs` 39，`host/live.rs` 10 | 49 | 见下 | 大 |
| 连接和运行目录 | `handoff.rs` 24，`host/runtime.rs` 20，`host/remote.rs` 6，`host/mod.rs` 4，`host/client.rs` 3，`codex_status.rs` 1 | 58 | 见下 | 大 |
| 进程识别 | `recognition/processes.rs` 10，`background_shells.rs` 6，`official.rs` 5，`agents/grok.rs` 4，`cli_commands.rs` 3，`host/client.rs` 5 | 33 | 见下 | 中到大 |
| 文件事件 | `file_events.rs` 46 | 46 | 换 `ReadDirectoryChangesW`。它没有 FSEvents 那种断点重放，启动时要补一次全量扫描 | 中 |
| 日志游标 | `log_index/update.rs` 10，`log_index/chat/backfill.rs` 2，`log_index.rs` 1 | 13 | `dev`/`ino` 换卷序列号和文件索引。std 里还不稳定，要调系统接口 | 小 |
| 进程退出 | `process_exit.rs` 11 | 11 | kqueue 换等进程句柄。接口小（`new`、`watch`） | 小 |
| 诊断 | `diagnostics.rs` 27，`host/client/tool_rows.rs` 2 | 29 | 全是 Mach 和 `proc_*` 统计。可以先报「不支持」，最后再做 | 中，可推迟 |
| 配对设备 | `devices.rs` 10 | 10 | 文件权限和锁，主机名换系统接口 | 小 |

编译器看不到、运行才错的：

- `baseline_env` 用 `env_clear` 加 macOS 名单。Windows 上要放行 `SystemRoot`、`USERPROFILE`、`APPDATA` 等，否则子进程可能起不来；`PATH` 也不能换成 `/usr/bin:...`，分隔符要用 `;`。
- host 里读 `HOME` 的有 13 处。
- `lsof` 有 2 处（`recognition.rs`、`background_shells.rs`），`/usr/bin/curl` 有 1 处（`plugin_hooks.rs`）。
- `agents/hooks.rs` 写进 Agent 设置的钩子是 POSIX sh 语句。没装 Git for Windows 时能不能跑，要实测，很可能得改成直接调 exe。
- shell 集成只有 zsh、bash、fish（97 行），要新写 PowerShell 版。默认 shell 也要重选。
- `shell_match.rs` 按 `zsh -c '… eval …'` 认后台命令，Git Bash 和 PowerShell 的形状不同。
- `action.rs` 的 `shell_quote` 是 POSIX 引号，到 PowerShell 或 cmd 上不安全。这是安全问题，要在开放启动 Agent 之前解决。

### 终端和读屏

`pty::spawn` 返回 `(std::process::Child, File)`，一个 `master` 同时读、写、改大小，host 到处直接用它。ConPTY 是两条管道加一个 `HPCON` 句柄，启动要自己调 `CreateProcessW`（std 在稳定版上传不了这个属性），所以要换成一个 `Pty` 类型，`Child` 也要换。

- `live.rs` 的读循环用 `poll` 带定时，定时负责「屏幕安静后读一次」。管道不能 `poll`，要改成读线程加通道。
- `pty::write` 靠 `POLLOUT` 超时防卡死，要改成写线程或重叠 I/O。
- 关闭靠 `tcgetpgrp` 加 `SIGHUP`，给 Agent 5 秒保存会话。Windows 上要用 `ClosePseudoConsole` 加 job 代替；`claude`、`codex` 收到关闭时是否也能存，要实测。
- 进程全退后管道不一定自己结束，要等进程句柄，再关 `HPCON`。
- `backlog` 已允许返回 None，可以不做。

最大的未知是读屏。host 读屏有两条路：

1. 原始字节：`command_marks.rs` 扫 OSC 133，`program_status.rs` 扫 OSC 7501，`Vt` 取 OSC 7。
2. 屏幕网格：`screen_rules.rs`（722 行）、`screen_ask.rs`、`host/screen_match.rs` 匹配 libghostty-vt 解析出的文字。

ConPTY 不是透传。conhost 先把程序输出画到自己的缓冲区，再生成新的 VT 序列（凭记忆，新版有透传改进，要看具体 Windows 版本）。所以：

- 第 1 条可能丢序列，尤其是自定义的 OSC 7501。
- 第 2 条仍然可行，但换行、宽字符、改大小时的重绘都可能和 macOS 不同。现有规则和样本都来自 macOS，要用 Windows 上的真实画面重录。
- Claude Code 没装 Git 时用 PowerShell 工具，画面和命令文字也会不同。

### 连接和交接

全部基于 `UnixStream`（`core` 的 `delivery.rs`、host 的 `remote.rs`、`events.rs`、`status.rs` 等）。要建一层传输类型，有两个选择：

- `AF_UNIX`（Win10 1803 起有，std 没封装，要加 crate 如 `uds_windows`）：改动最小，路径和 `Read`/`Write` 都保留。我倾向这个，没验证。
- 命名管道：Windows 的正路，但超时、`try_clone`、`MSG_PEEK` 都要重写。

身份检查（`geteuid`、`getpeereid`、`0700`、`is_socket`）要换成 ACL 和对端进程查询，这是安全边界，不能留空。

`handoff.rs` 用 `SCM_RIGHTS` 把 PTY 交给新 host，这样升级时 shell 和 Agent 不断。Windows 没有 `SCM_RIGHTS`；管道句柄能 `DuplicateHandle`，但 `HPCON` 没有跨进程移交的公开方式（凭记忆）。这要用户定：

- A. 第一版不交接：升级 host 就结束所有终端。最省事，但少了 Mooring 的一个核心承诺。
- B. 每个终端一个小的持有进程拿着 `HPCON`，host 重启后重新连上。能保住承诺，多一种进程和一套协议。

建议先做 A，放到第 5 步再决定要不要 B。

### 进程识别

| macOS 现在 | Windows 候选 | 问题 |
|---|---|---|
| `proc_listchildpids` 只走 PTY 下的树 | Toolhelp32 快照 | 是全局快照，和现在「不做全局扫描」的原则冲突，频率要限制 |
| `KERN_PROCARGS2` 取 argv | 进程命令行，再 `CommandLineToArgvW` | 是一整串，`.cmd` 垫片多一层 |
| 同上取环境变量（`claude_transcript_off`、`env_value`） | 没有公开接口，只能读 PEB | 很可能要换判断方式 |
| `proc_vnodepathinfo` 取 cwd | 同样只能读 PEB | 尽量改由 shell 集成和钩子上报 |
| `lsof` 查打开的日志和端口 | 端口用 `GetExtendedTcpTable`；打开的文件没有好办法 | 绑定要多靠钩子 |

### 测试

host 单元测试约 343 个，集成测试约 203 个，共约 16000 行集成测试。48 个文件含 Unix 写法，27 个 `tests` 文件依赖 `/bin/sh`，假 Agent 都是 sh 脚本。先把能跑的纯逻辑测试挑出来，用 `cfg(unix)` 关掉其余的并记数，再用 PowerShell 写少量真 ConPTY 用例。不要追求全部移植。

## 建议顺序

大小是粗估，不是承诺。

| 步 | 内容 | 需要真机吗 | 大小 |
|---|---|---|---|
| 0 | core 收尾：`home_dir`、路径分隔、`secret` 接凭据管理器；在 GitHub 的 Windows runner 上跑 `mooring-core` 测试 | 否 | 小 |
| 1 | host 编译骨架：249 个错误全部过，做不到的明确报错；CI 交叉检查扩到 host | 否 | 中 |
| 2 | **ConPTY 验证**：一个小示例，真 ConPTY 接 libghostty-vt，检查 OSC 133/7/7501、宽字符、改大小；再录真实的 `claude`、`codex` 画面 | **是** | 小，但决定方向 |
| 3 | `Pty` 和传输层，host 在 Windows 上跑起来，`mooring list/read/send` 通，手机和 Mac 窗口经远程协议连上 | 是 | 大 |
| 4 | 文件事件、日志游标、进程退出、识别、钩子、PowerShell 集成：Agent 状态可信 | 是 | 大 |
| 5 | 决定交接 A 或 B；诊断；原生窗口（`mooring-app`）另算 | 是 | 大 |

第 3 步之后就有一个可用的里程碑：Windows 机器跑无窗口的 host，从手机或 Mac 窗口看和操作。`relay`、`remote` 已能编译，窗口可以最后做。这一点我只看了编译和 `Cargo.toml` 的说明，没验证过。

第 2 步的放弃条件：如果读屏规则在 ConPTY 下靠不住，状态判断只能退到钩子和日志，这时要重新评估。OSC 丢失本身不算放弃，可以改由钩子上报。

## 要在真机上测的

第 2 步第一次需要用户的 Windows 电脑。runner 能跑 ConPTY，但没有登录过的 Agent，也不该把凭据放上去。第 0、1 步都不需要。

- 先告诉我：Windows 版本号（`winver`）、是否装了 Git for Windows、是否装了 PowerShell 7、CPU 架构。
- 第 2 步：`claude` 和 `codex` 真实登录后的画面，问答框、权限框、在跑的那一行，装和不装 Git 各一次。
- 第 3 步：host 升级和重启、休眠唤醒、路径含空格和中文、关闭终端时 Agent 是否保存了会话。
- 第 4 步：钩子真的被调用；杀毒软件和文件占用下的重命名。
- core 这一步：job object 真的能结束子进程树、`exists`、时区，这些 runner 能先验。

## 外部事实

- Claude Code（<https://code.claude.com/docs/en/setup>）：原生支持 Windows 10 1809+。Git for Windows 可选，提供 Bash 工具用的 Git Bash；没有时用 PowerShell 工具。原生 Windows 不支持沙盒，WSL 2 才支持。
- Codex（<https://developers.openai.com/codex/windows>，现在跳到 learn.chatgpt.com 的 windows-sandbox 页）：写的是「Use the native Windows sandbox by default」，需要 Linux 工具链时才建议 WSL。沙盒有 `mxc`、`elevated`、`unelevated` 三种。Windows 10 要 1809+ 和 ConPTY。

所以最低版本可以定 Windows 10 1809，但 ConPTY 在老版本上的问题多（凭记忆），是否自带新版 `conpty.dll` 看第 2 步。

## 未解决的风险

1. ConPTY 改写输出，读屏规则可能要分平台。这是最大的风险，第 2 步回答。
2. Codex 的原生沙盒可能挡住钩子访问 host 的连接，或让它的子进程不在我们认识的树里。没有查证。
3. 文件占用：Windows 上 `rename` 碰到打开的文件会失败，涉及 `plugin_context.rs` 的 `runs.jsonl`、`market_install.rs` 的目录替换，以及 App 自更新替换正在跑的 exe。
4. job 的时间窗：极快启动子进程的命令可能漏在 job 外。std 稳定后再用启动属性解决。
5. `pty.rs`、`live.rs` 已有 600 多行，加 Windows 代码会碰 800 行上限，应该放进新文件。
6. CI 上的 macOS runner 紧张，Windows 检查不要放进 `ci.yml`：改那个文件会让它的缓存键失效。
