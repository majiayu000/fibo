//! Driving Mooring terminals from scripts and other Agents: list them, read a screen, send text, prompt an Agent and wait.
use mooring_core::status::{self, Status};
use mooring_core::{model::*, protocol::*};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// One `mooring` command: the flags it takes and its lines in the usage text. The full
/// usage is put together from this table, `-h`/`--help` prints a command's own lines,
/// and a test checks that each command's lines name exactly its flags, so what is
/// documented and what is accepted cannot drift apart again. `--runtime <目录>` is
/// accepted by every command and is not listed.
pub(crate) struct Command {
    pub(crate) name: &'static str,
    /// Flags followed by a value.
    pub(crate) valued: &'static [&'static str],
    /// Flags that stand alone.
    pub(crate) switches: &'static [&'static str],
    /// The command's lines: each starts `mooring <name>`, descriptions are indented.
    pub(crate) usage: &'static str,
}
pub(crate) const COMMANDS: &[Command] = &[
    Command {
        name: "list",
        valued: &[],
        switches: &["json"],
        usage: "\
mooring list [--json]                      终端列表：编号、Agent 状态、标签、目录",
    },
    Command {
        name: "read",
        valued: &["lines"],
        switches: &[],
        usage: "\
mooring read <终端> [--lines N]            画面最后 N 行（默认 50）",
    },
    Command {
        name: "send",
        valued: &[],
        switches: &["enter"],
        usage: "\
mooring send <终端> <文字|-> [--enter]     把文字作为粘贴放进终端；- 从标准输入读（最多 64 KiB）",
    },
    Command {
        name: "prompt",
        valued: &["timeout"],
        switches: &["wait", "force"],
        usage: "\
mooring prompt <终端> <文字|-> [--wait] [--timeout 秒] [--force]
                                             给 Agent 发一条并回车；它正等你回答时拒绝，除非 --force",
    },
    Command {
        name: "wait",
        valued: &["until", "timeout"],
        switches: &[],
        usage: "\
mooring wait <终端> [--until idle|waiting|working] [--timeout 秒]
                                             等 Agent 进入某个状态（默认 idle：这一轮做完）",
    },
    Command {
        name: "explain",
        valued: &["file", "agent"],
        switches: &["json"],
        usage: "\
mooring explain --file <画面.txt> --agent <claude|codex> [--json]
mooring explain <终端> [--json]            画面规则怎么读这块屏：状态、命中的规则、规则版本和来源、看到的行",
    },
    Command {
        name: "events",
        valued: &[],
        switches: &["json"],
        usage: "\
mooring events [--json]                    先列出现在的状态，再每有变化输出一行，直到 Ctrl-C：
                                             started / waiting / error / done、approval / approval_gone、
                                             terminal_started / terminal_exited、command_done / command_failed；
                                             lagged 表示跟不上丢了事件，应重新开一次取现状",
    },
    Command {
        name: "split",
        valued: &["size", "cwd"],
        switches: &["down", "tab", "here", "json"],
        usage: "\
mooring split <终端> [--down] [--size 0.5] [--tab|--here] [--cwd 目录] [--json] [-- 命令…]
                                             在这个终端右边（--down 下面）开一块新分屏，打印新终端编号；
                                             标签已有 4 块或切开太窄时开到「旁屏」标签（--here 改为拒绝，
                                             --tab 直接开到旁屏）；给了命令就在新 shell 里跑，跑完留在 shell",
    },
    Command {
        name: "run",
        valued: &["cwd", "from", "timeout"],
        switches: &["split", "no-report", "json"],
        usage: "\
mooring run <agent> <任务|-> [--split] [--cwd 目录] [--no-report] [--from <终端>] [--timeout 秒] [--json]
                                             派一个 Agent 去做事：开在旁屏标签（--split 开在旁边），等它出现输入框
                                             再交任务；它每结束一轮，结果自动发回你的输入框（<mooring from=…>）；
                                             不在 Mooring 终端里时用 --from 说开在谁旁边、汇报给谁",
    },
    Command {
        name: "close",
        valued: &[],
        switches: &[],
        usage: "\
mooring close <终端>                       关掉这块分屏并结束里面的程序",
    },
    Command {
        name: "remote",
        valued: &[
            "relay",
            "relay-key",
            "name",
            "role",
            "revoke",
            "device",
            "kick",
        ],
        switches: &[
            "stdio", "pair", "new-code", "devices", "enable", "disable", "status", "report",
        ],
        usage: "\
mooring remote --stdio                     给另一台机器或手机 App 用：一行一个 JSON 请求和回复（看画面、打字、按键、
                                             答审批、事件），经 ssh 连过来：ssh 主机 mooring remote --stdio
mooring remote --relay <中继地址> [--relay-key <密钥>] [--report]
                                             连上中继并一直挂着，配对过的手机在哪都能连过来；内容端到端加密，
                                             中继看不到。中继要密钥时也可以设 MOORING_RELAY_KEY；
                                             --report 把连接状态报给后台（后台自己启动它时用）
mooring remote --pair --relay <中继地址> [--name 名字] [--role view|answer|full] [--new-code]
                                             配对一台新手机，打印它的二维码和链接；权限分只看 / 能答审批 / 完全控制（默认）。
                                             加 --new-code 换掉机器码，已配对的手机全部失效
mooring remote --enable --relay <中继地址> [--relay-key <密钥>] | --disable
                                             让后台一直挂在中继上（重启后也会自动连回去），或者断开；配对保留
mooring remote --status                    中继连接状态、配对的手机、现在谁在线、在看哪个终端
mooring remote --kick <设备>               立刻断开这台手机正连着的会话，配对保留
mooring remote --devices                   列出配对过的手机、权限和最近连接时间；操作记在运行目录的 remote-log.jsonl
mooring remote --device <设备> --role <view|answer|full>
                                             改一台手机的权限；它正连着的会话一秒内断开，重连后按新权限
mooring remote --revoke <设备编号或名字>   取消一台手机的配对，它正连着的会话一秒内断开",
    },
    Command {
        name: "plugins",
        valued: &["dir"],
        switches: &["json", "remote", "check"],
        usage: "\
mooring plugins [--dir 目录] [--json]      检查插件能不能装上：哪个生效、哪个为什么没生效
mooring plugins available [--remote]       列出可安装的插件（App 附带的，加 --remote 再查远程目录）
mooring plugins install <id> [--remote]    安装或更新一个插件
mooring plugins uninstall <id>             卸载一个插件
mooring plugins index <目录> [--check]     为插件目录生成 index.json（--check 只检查是否最新）",
    },
    Command {
        name: "hooks",
        valued: &["agent"],
        switches: &["json", "outdated-only"],
        usage: "\
mooring hooks install|uninstall [--agent <cli>]
                                             把 Mooring 的 hook 写进（或移出）Agent CLI 自己的设置；不写 --agent 就是全部
mooring hooks status [--agent <cli>] [--json] [--outdated-only]
                                             各 CLI 的 hook 装了没有、和现在 install 会写的差在哪",
    },
    Command {
        name: "hook",
        valued: &["agent"],
        switches: &[],
        usage: "\
mooring hook --agent <claude|codex|grok>   Agent CLI 的 hook 调用的入口：从标准输入读一条 hook JSON（最多 1 MiB）
                                             报给后台；不在 Mooring 终端里什么都不做。不用手动跑",
    },
    Command {
        name: "serve",
        valued: &["recent-days"],
        switches: &["takeover"],
        usage: "\
mooring serve --runtime <目录> [--takeover] [--recent-days N]
                                             启动后台（App 自己会启动，不用手动跑）；--takeover 从正在跑的后台
                                             接过全部终端，--recent-days 只读最近 N 天的会话（默认 7）",
    },
    Command {
        name: "scan",
        valued: &[],
        switches: &[],
        usage: "\
mooring scan --runtime <目录>              让后台重新扫一遍 Agent 会话，打印会话数和终端数",
    },
    Command {
        name: "--skill",
        valued: &[],
        switches: &[],
        usage: "\
mooring --skill                            给 Coding Agent 看的用法说明（Markdown）
mooring --skill plugins                    给 Coding Agent 看的写插件、改外观说明（Markdown）",
    },
    Command {
        name: "--version",
        valued: &[],
        switches: &[],
        usage: "\
mooring --version                          打印协议版本",
    },
];
const FOOTER: &str = "\
<终端> 可以是编号的前几位或标签名；全部命令都接受 --runtime <目录>；-h / --help 只打印用法、什么都不做。
Agent 藏在包装器（沙盒、虚拟机、nono 等）后面认不出时，启动前设 MOORING_AGENT=<claude|codex|…>。";
/// Every command's lines, then what holds for all of them.
pub(crate) fn usage() -> String {
    let mut text: String = COMMANDS.iter().map(|c| format!("{}\n", c.usage)).collect();
    text.push_str(FOOTER);
    text
}
pub(crate) fn command(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}
fn help_flag(arg: &str) -> bool {
    arg == "-h" || arg == "--help"
}
/// Checks `args` against what `cmd` takes, before anything acts: `Ok(true)` when they
/// ask for help (`-h`/`--help` anywhere before a bare `--`), an error naming the flag
/// and showing the command's usage for an unknown flag or a missing value.
pub(crate) fn check(cmd: &Command, args: &[String]) -> io::Result<bool> {
    let mut args = args.iter();
    let mut wrong = None;
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if help_flag(arg) {
            return Ok(true);
        }
        let Some(name) = arg.strip_prefix("--") else {
            continue;
        };
        if name == "runtime" || cmd.valued.contains(&name) {
            match args.next() {
                Some(v) if help_flag(v) => return Ok(true),
                Some(_) => (),
                None => {
                    wrong.get_or_insert_with(|| format!("--{name} 需要一个值"));
                }
            }
        } else if !cmd.switches.contains(&name) {
            wrong.get_or_insert_with(|| format!("mooring {} 没有 --{name} 这个选项", cmd.name));
        }
    }
    match wrong {
        Some(e) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{e}\n用法：\n{}", cmd.usage),
        )),
        None => Ok(false),
    }
}
/// `mooring --skill`: how a coding Agent drives these commands, and when it must not.
pub const SKILL: &str = include_str!("skill.md");
/// `mooring --skill plugins`: how a coding Agent changes Mooring with plugins.
pub const PLUGINS_SKILL: &str = include_str!("plugins.md");

/// What the Agent in a terminal is doing, read like the workbench's pane states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Working,
    Waiting,
    Idle,
    /// No Agent conversation is current in this terminal: a shell or another program.
    None,
}
impl Phase {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Idle => "idle",
            Self::None => "-",
        }
    }
}
/// Read from `status::of`, the one place a session's state is decided, so `list`,
/// `wait` and the phone never disagree with the window.
pub fn phase(w: &Workspace, terminal: &str) -> Phase {
    match status(w, terminal).map(Status::phase) {
        None => Phase::None,
        Some(status::Phase::Working) => Phase::Working,
        Some(status::Phase::Waiting) => Phase::Waiting,
        Some(status::Phase::Idle) => Phase::Idle,
    }
}
/// The state of the conversation current in a terminal, as the window shows it.
pub fn status(w: &Workspace, terminal: &str) -> Option<Status> {
    w.current_agent(terminal)
        .map(|s| status::of(w, s, mooring_core::content::now()))
}
/// The registry key in lower case, the way the CLI is usually typed.
pub(crate) fn agent_label(agent: mooring_core::agent::Agent) -> String {
    agent.key().to_lowercase()
}
pub(crate) fn tab_title(w: &Workspace, terminal: &str) -> String {
    w.terminal_tab_title(terminal)
}
/// A full id, a unique id prefix, or a unique tab title, among running terminals.
pub fn resolve(w: &Workspace, needle: &str) -> io::Result<String> {
    let running: Vec<&Terminal> = w
        .terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        .collect();
    if let Some(t) = running.iter().find(|t| t.id == needle) {
        return Ok(t.id.clone());
    }
    let by_prefix: Vec<_> = running
        .iter()
        .filter(|t| t.id.starts_with(needle))
        .collect();
    let found: Vec<_> = if by_prefix.is_empty() {
        running
            .iter()
            .filter(|t| tab_title(w, &t.id) == needle)
            .collect()
    } else {
        by_prefix
    };
    match found.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(io::Error::other(format!("没有正在运行的终端匹配 {needle}"))),
        many => Err(io::Error::other(format!(
            "{needle} 匹配到 {} 个终端，请写更长的编号",
            many.len()
        ))),
    }
}

pub(crate) struct Args {
    pub(crate) socket: PathBuf,
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
    /// Everything after a bare `--`, untouched: a command and its own flags.
    rest: Vec<String>,
    /// The command's own usage lines, for errors about its arguments.
    usage: &'static str,
}
impl Args {
    /// Rejects a flag `cmd` does not take before anything acts; help is answered by the
    /// caller (`main`) before this runs.
    pub(crate) fn parse(args: impl Iterator<Item = String>, cmd: &Command) -> io::Result<Self> {
        let args: Vec<String> = args.collect();
        if check(cmd, &args)? {
            return Err(io::Error::other(format!("用法：\n{}", cmd.usage)));
        }
        let valued = cmd.valued;
        let mut runtime = std::env::var_os("MOORING_RUNTIME").map(PathBuf::from);
        let mut positional = vec![];
        let mut flags = vec![];
        let mut rest = vec![];
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--" {
                rest.extend(args.by_ref());
            } else if arg == "--runtime" {
                runtime = Some(
                    args.next()
                        .ok_or_else(|| io::Error::other("--runtime 需要目录"))?
                        .into(),
                );
            } else if let Some(name) = arg.strip_prefix("--") {
                let value = if valued.contains(&name) {
                    Some(
                        args.next()
                            .ok_or_else(|| io::Error::other(format!("--{name} 需要一个值")))?,
                    )
                } else {
                    None
                };
                flags.push((name.to_owned(), value));
            } else {
                positional.push(arg);
            }
        }
        let runtime = match runtime {
            Some(r) => r,
            // Where the app keeps its host, so a script needs no flag next to it.
            // An install from before the rename keeps serving the old directory.
            None => mooring_core::legacy::runtime_dir(&PathBuf::from(
                std::env::var_os("HOME").ok_or_else(|| io::Error::other("HOME unavailable"))?,
            )),
        };
        Ok(Self {
            socket: runtime.join("workbench.sock"),
            positional,
            flags,
            rest,
            usage: cmd.usage,
        })
    }
    pub(crate) fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(n, _)| n == name)
    }
    pub(crate) fn value(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| v.as_deref())
    }
    fn number(&self, name: &str, default: u64) -> io::Result<u64> {
        self.value(name)
            .map(|v| {
                v.parse()
                    .map_err(|_| io::Error::other(format!("--{name} 需要数字")))
            })
            .unwrap_or(Ok(default))
    }
    fn arg(&self, i: usize, what: &str) -> io::Result<&str> {
        self.positional
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| io::Error::other(format!("缺少{what}\n用法：\n{}", self.usage)))
    }
    /// `-` reads the text from standard input, so long prompts need no shell quoting.
    fn text(&self, i: usize) -> io::Result<String> {
        let text = self.arg(i, "文字")?;
        if text != "-" {
            return Ok(self.positional[i..].join(" "));
        }
        read_limited(io::stdin(), STDIN_LIMIT).map(|s| s.trim_end_matches('\n').to_owned())
    }
}
/// The most `-` reads from standard input; more is an error, never a silent cut.
const STDIN_LIMIT: usize = 64 * 1024;
fn read_limited(input: impl Read, limit: usize) -> io::Result<String> {
    let mut bytes = Vec::new();
    input.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::other(format!(
            "标准输入超过 {} KiB，没有发送；拆成几段再发",
            limit / 1024
        )));
    }
    String::from_utf8(bytes).map_err(|_| io::Error::other("标准输入不是 UTF-8 文字"))
}
fn workspace(socket: &Path) -> io::Result<Workspace> {
    match rpc(socket, &Request::Workspace)? {
        Reply::Workspace(w) => Ok(w),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
fn expect_ok(reply: Reply) -> io::Result<()> {
    match reply {
        Reply::Ok => Ok(()),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

/// The commands this module carries out; `main` sends each of them here.
pub(crate) const DRIVES: &[&str] = &[
    "list", "read", "send", "prompt", "wait", "explain", "events", "split", "run", "close",
];
pub fn run(cmd: &Command, args: impl Iterator<Item = String>) -> io::Result<()> {
    let a = Args::parse(args, cmd)?;
    match cmd.name {
        "list" => list(a),
        "read" => read(a),
        "send" => send(a),
        "prompt" => prompt(a),
        "wait" => wait_cmd(a),
        "explain" => explain(a),
        "events" => events(a),
        "split" => split(a),
        "run" => run_agent(a),
        "close" => close(a),
        _ => Err(io::Error::other(usage())),
    }
}
/// `explain`: replays a saved screen, or a live terminal's, through the
/// screen rules and says what they decided and from what.
fn explain(a: Args) -> io::Result<()> {
    use mooring_host::screen_rules;
    let (agent, screen, daemon) = match a.value("file") {
        Some(file) => {
            let name = a.value("agent").ok_or_else(|| {
                io::Error::other(format!("--file 要配 --agent\n用法：\n{}", a.usage))
            })?;
            let agent = mooring_core::agent::Agent::all()
                .into_iter()
                .find(|g| agent_label(*g) == name.to_lowercase())
                .ok_or_else(|| io::Error::other(format!("不认识的 Agent：{name}")))?;
            (agent, std::fs::read_to_string(file)?, None)
        }
        None => {
            let w = workspace(&a.socket)?;
            let id = resolve(&w, a.arg(0, "终端或 --file")?)?;
            let s = w
                .current_agent(&id)
                .ok_or_else(|| io::Error::other("这个终端里没有可识别的 Agent 对话"))?;
            // The daemon reads the visible rows; 200 covers the tallest screen it allows.
            let screen = match rpc(&a.socket, &Request::ReadScreen { id, rows: 200 })? {
                Reply::Text(text) => text,
                other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
            };
            (s.runtime, screen, w.content.get(&s.id).cloned())
        }
    };
    let e = screen_rules::explain(screen_rules::rules(agent), &screen);
    if a.has("json") {
        let mut json = e.to_json();
        if let Some(c) = &daemon {
            json["daemon"] = serde_json::json!({
                // TODO: this says Running/Waiting/Completed (the content turn) while
                // `list` says working/waiting/idle (`Phase::name`); settle on one wording
                // when `phase` is reworked.
                "phase": format!("{:?}", c.turn),
                "signal": c.signal,
                "signal_detail": c.signal_detail,
                "screen_check": c.screen_check,
            });
        }
        let text = serde_json::to_string_pretty(&json).map_err(io::Error::other)?;
        println!("{text}");
        return Ok(());
    }
    println!("状态：{}", e.state());
    match e.rule() {
        Some(r) => println!("规则：{}（{}）", r.id, r.name),
        None => println!("规则：没有命中"),
    }
    let source = match &e.rules.source {
        screen_rules::Source::Bundled => "内置".to_owned(),
        screen_rules::Source::Override(path) => format!("覆盖文件 {}", path.display()),
    };
    println!("规则集：{} {}，{source}", e.rules.id, e.rules.version);
    match &e.found {
        Ok(hit) => {
            println!("问题：{}", hit.question);
            println!("看到的画面：");
            for line in &hit.evidence {
                println!("  {line}");
            }
        }
        Err(misses) => {
            println!("原因：{}", e.fallback());
            println!("各规则：");
            for (m, r) in misses.iter().zip(&e.rules.rules) {
                println!("  {} {}：{}", r.id, r.name, m.describe(r));
            }
        }
    }
    if let Some(c) = daemon {
        println!("守护进程现在：{:?}，依据 {}", c.turn, c.signal);
        for line in c.signal_detail.lines().chain(c.screen_check.lines()) {
            println!("  {line}");
        }
    }
    Ok(())
}
/// Event subscription: the state now, then one line per change until the host
/// goes away or the reader stops reading.
fn events(a: Args) -> io::Result<()> {
    let mut stream = std::os::unix::net::UnixStream::connect(&a.socket)?;
    mooring_core::protocol::send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
    match receive::<Reply>(&mut stream)? {
        Reply::Hello { major: MAJOR, .. } => (),
        r => return Err(io::Error::other(format!("handshake: {r:?}"))),
    }
    mooring_core::protocol::send(&mut stream, CONTROL, &Request::Subscribe)?;
    let json = a.has("json");
    loop {
        let notices = match receive::<Reply>(&mut stream)? {
            Reply::Events(n) => n,
            Reply::Error(e) => return Err(io::Error::other(e)),
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        };
        for n in notices {
            if json {
                println!("{}", serde_json::to_string(&n).map_err(io::Error::other)?);
                continue;
            }
            let terminal: String = n
                .terminal
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(8)
                .collect();
            let what = [n.title.as_str(), n.detail.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let code = n
                .exit_code
                .map(|c| format!(" 退出码 {c}"))
                .unwrap_or_default();
            println!("{:<16} {terminal:<8} {what}{code}", n.event);
        }
    }
}
type Row<'a> = (&'a Terminal, Option<&'a AgentSession>, Phase, String);
fn list_rows(w: &Workspace) -> Vec<Row<'_>> {
    w.terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        .map(|t| {
            let s = w.current_agent(&t.id);
            (t, s, phase(w, &t.id), tab_title(w, &t.id))
        })
        .collect()
}
/// `mooring list --json`: every field read from its one owner in core.
fn list_json(w: &Workspace) -> Vec<serde_json::Value> {
    list_rows(w)
        .iter()
        .map(|(t, s, phase, tab)| {
            serde_json::json!({
                "id": t.id, "cwd": w.terminal_cwd(t), "tab": tab,
                "agent": s.map(|s| agent_label(s.runtime)),
                "phase": phase.name(),
                "status": status(w, &t.id).map(Status::key),
                "status_label": status(w, &t.id).map(Status::label),
                "title": s.map(|s| w.session_title(s)),
                "session": s.map(|s| &s.cli_session_id),
            })
        })
        .collect()
}
fn list(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let rows = list_rows(&w);
    if a.has("json") {
        let json = list_json(&w);
        println!(
            "{}",
            serde_json::to_string_pretty(&json).map_err(io::Error::other)?
        );
        return Ok(());
    }
    // Full ids: terminals opened by one app share a long prefix, so a cut-off id names none.
    let id_width = rows.iter().map(|r| r.0.id.len()).max().unwrap_or(0);
    let tab_width = rows
        .iter()
        .map(|r| r.3.chars().count().max(1))
        .max()
        .unwrap_or(1);
    for (t, s, phase, tab) in rows {
        let agent = s.map_or_else(|| "-".into(), |s| agent_label(s.runtime));
        let tab = if tab.is_empty() { "-".to_owned() } else { tab };
        let pad = " ".repeat(tab_width - tab.chars().count());
        println!(
            "{:<id_width$}  {:<6}  {:<7}  {tab}{pad}  {}",
            t.id,
            agent,
            phase.name(),
            w.terminal_cwd(t)
        );
    }
    Ok(())
}
fn read(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, "终端")?)?;
    let rows = a.number("lines", 50)? as usize;
    match rpc(
        &a.socket,
        &Request::ReadScreen {
            id,
            rows: rows + 200,
        },
    )? {
        Reply::Text(text) => {
            // The screen's empty rows below the cursor say nothing.
            let lines: Vec<_> = text.trim_end().lines().collect();
            println!("{}", lines[lines.len().saturating_sub(rows)..].join("\n"));
            Ok(())
        }
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
fn send(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, "终端")?)?;
    let text = a.text(1)?;
    expect_ok(rpc(
        &a.socket,
        &Request::SendText {
            id,
            text,
            submit: a.has("enter"),
        },
    )?)
}
fn prompt(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, "终端")?)?;
    let text = a.text(1)?;
    match phase(&w, &id) {
        Phase::None => {
            return Err(io::Error::other(
                "这个终端里没有可识别的 Agent 对话；发给 shell 请用 send",
            ))
        }
        // Typing into an approval or a choice would answer it, so refuse.
        Phase::Waiting if !a.has("force") => {
            return Err(io::Error::other(
                "Agent 正在等你回答（审批或选择题），没有发送；确认要发请加 --force",
            ));
        }
        _ => (),
    }
    let started = Instant::now();
    expect_ok(rpc(
        &a.socket,
        &Request::SendText {
            id: id.clone(),
            text,
            submit: true,
        },
    )?)?;
    if a.has("wait") {
        let timeout = Duration::from_secs(a.number("timeout", 1800)?);
        let done = wait(
            &a.socket,
            &id,
            &[Phase::Idle, Phase::Waiting],
            timeout,
            started,
        )?;
        println!("{}", done.name());
    }
    Ok(())
}
fn wait_cmd(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, "终端")?)?;
    let target = match a.value("until").unwrap_or("idle") {
        "idle" => Phase::Idle,
        "waiting" => Phase::Waiting,
        "working" => Phase::Working,
        other => {
            return Err(io::Error::other(format!(
                "--until 只接受 idle|waiting|working，不是 {other}"
            )))
        }
    };
    let timeout = Duration::from_secs(a.number("timeout", 1800)?);
    let done = wait(&a.socket, &id, &[target], timeout, Instant::now())?;
    println!("{}", done.name());
    Ok(())
}
/// Polls until the Agent reaches one of `targets`. Right after a prompt the state may
/// still read idle from before it, so idle only counts once the turn started or after
/// a grace period with no sign of work.
fn wait(
    socket: &Path,
    id: &str,
    targets: &[Phase],
    timeout: Duration,
    since: Instant,
) -> io::Result<Phase> {
    const GRACE: Duration = Duration::from_secs(20);
    let since_secs = epoch_secs(since);
    let mut started = !targets.contains(&Phase::Idle);
    loop {
        let w = workspace(socket)?;
        if !w
            .terminals
            .iter()
            .any(|t| t.id == id && t.state == ProcessState::Running)
        {
            return Err(io::Error::other("终端已退出"));
        }
        let now = phase(&w, id);
        started |= now == Phase::Working
            || w.current_agent(id).is_some_and(|s| {
                w.content
                    .get(&s.id)
                    .is_some_and(|c| c.updated >= since_secs)
            });
        if targets.contains(&now) && (now != Phase::Idle || started || since.elapsed() >= GRACE) {
            return Ok(now);
        }
        if since.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("等待超时，当前 {}", now.name()),
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
/// A tab holds at most this many panes opened from outside the app; past it, or when a
/// split would leave a half too small, the pane goes to a side tab instead.
const MAX_PANES: usize = 4;
/// Side tabs are named after the terminal they serve: `side~<terminal>`, then `~2`, `~3`.
const SIDE: &str = "side~";

/// Where a new pane goes: beside a pane, or into a side tab that does not exist yet.
enum Placement {
    Beside {
        target: String,
        axis: Axis,
        share: u16,
        cols: u16,
        rows: u16,
    },
    NewTab {
        id: String,
        title: String,
    },
}
fn split_room(socket: &Path, target: &str, axis: Axis, share: u16) -> io::Result<(u16, u16)> {
    match rpc(
        socket,
        &Request::SplitRoom {
            target: target.to_owned(),
            axis,
            share,
        },
    )? {
        Reply::Size { cols, rows } => Ok((cols, rows)),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
/// The terminal a tab's side tabs belong to: its top-left pane, where the user started.
/// A side pane split again fills the same side tabs rather than starting a chain.
fn side_owner(w: &Workspace, tab: &Tab, terminal: &str) -> String {
    if let Some(owner) = tab.id.strip_prefix(SIDE).and_then(|r| r.split('~').next()) {
        return owner.to_owned();
    }
    w.tab_terminals(&tab.id)
        .into_iter()
        .next()
        .unwrap_or_else(|| terminal.to_owned())
}
/// The first side tab of `owner` with room for one more pane, beside its last pane,
/// right if that fits and down otherwise; or the next side tab to open.
fn side_placement(
    socket: &Path,
    w: &Workspace,
    owner: &str,
    asking: &str,
) -> io::Result<Placement> {
    // Named for the Agent asking, or the one where the user started.
    let name = [asking, owner]
        .iter()
        .find_map(|t| w.current_agent(t))
        .map(|s| s.runtime.name().to_owned())
        .unwrap_or_else(|| "终端".into());
    // 「Claude 的旁屏」, 「终端的旁屏」.
    let gap = if name.ends_with(|c: char| c.is_ascii()) {
        " "
    } else {
        ""
    };
    for n in 1.. {
        let id = if n == 1 {
            format!("{SIDE}{owner}")
        } else {
            format!("{SIDE}{owner}~{n}")
        };
        let Some(tab) = w.tabs.iter().find(|t| t.id == id) else {
            let title = if n == 1 {
                format!("{name}{gap}的旁屏")
            } else {
                format!("{name}{gap}的旁屏 {n}")
            };
            return Ok(Placement::NewTab { id, title });
        };
        let panes = w.tab_terminals(&tab.id);
        let Some(last) = panes.last().filter(|_| panes.len() < MAX_PANES) else {
            continue;
        };
        for axis in [Axis::Right, Axis::Down] {
            if let Ok((cols, rows)) = split_room(socket, last, axis, 5000) {
                return Ok(Placement::Beside {
                    target: last.clone(),
                    axis,
                    share: 5000,
                    cols,
                    rows,
                });
            }
        }
    }
    unreachable!("there is always a next side tab")
}

/// `split`: a new shell beside a terminal, in its
/// tab, in the directory that shell is in now. New panes only go right or down, so the
/// pane the user started in keeps the top-left corner. A tab that already has four panes,
/// or a split that would leave a half too small, sends the pane to a side tab named after
/// that terminal instead (`--here` refuses then; `--tab` always goes there). The
/// workbench shows it as soon as it next reads the host and never moves the keyboard or
/// the shown tab for it.
/// `mooring run <agent> <任务|->`: starts an Agent for the terminal calling, hands it the
/// task once its input is ready, and has the host report each turn it ends back into the
/// caller's input. The new terminal goes to the caller's side tab unless `--split`.
fn run_agent(a: Args) -> io::Result<()> {
    use mooring_core::agent::Agent;
    let key = a.arg(0, "Agent")?.to_lowercase();
    let agent = Agent::all()
        .into_iter()
        .find(|g| agent_label(*g) == key || g.executable() == key)
        .ok_or_else(|| {
            let known: Vec<String> = Agent::all().into_iter().map(agent_label).collect();
            io::Error::other(format!("不认识的 Agent：{key}；可选：{}", known.join("、")))
        })?;
    let task = a.text(1)?;
    if task.trim().is_empty() {
        return Err(io::Error::other("任务是空的"));
    }
    let from = match a.value("from") {
        Some(from) => from.to_owned(),
        None => std::env::var("MOORING_TERMINAL").map_err(|_| {
            io::Error::other("不在 Mooring 终端里：用 --from <终端> 说要开在谁旁边、汇报给谁")
        })?,
    };
    let parent = resolve(&workspace(&a.socket)?, &from)?;
    let opened = open(&a, &parent, &[agent.command()], !a.has("split"))?;
    let report = !a.has("no-report");
    if report {
        expect_ok(rpc(
            &a.socket,
            &Request::ReportTo {
                child: opened.id.clone(),
                parent: parent.clone(),
            },
        )?)?;
    }
    let timeout = Duration::from_secs(a.number("timeout", 90)?);
    if let Err(e) = hand_over(&a.socket, &opened.id, &task, timeout) {
        return Err(io::Error::other(format!(
            "{e}（终端 {} 留着，mooring read {} 看画面）",
            opened.id, opened.id
        )));
    }
    if a.has("json") {
        println!(
            "{}",
            serde_json::json!({
                "id": opened.id,
                "agent": agent_label(agent),
                "tab": opened.tab,
                "tab_title": opened.tab_title,
                "beside": opened.beside,
                "report_to": report.then_some(&parent),
            })
        );
    } else {
        println!("{}", opened.id);
        let place = if opened.beside.is_some() {
            "开在旁边".to_owned()
        } else {
            format!("开在「{}」标签（不切换用户的画面）", opened.tab_title)
        };
        let back = if report {
            "；它每结束一轮，结果会以 <mooring from=…> 自动发回你的输入框，不用轮询"
        } else {
            ""
        };
        eprintln!("{} 已经收到任务，{place}{back}", agent.name());
    }
    Ok(())
}
/// Types `text` into the Agent starting in `id` once its input box shows and the screen
/// has held still, as the workbench does with a todo's text.
fn hand_over(socket: &Path, id: &str, text: &str, timeout: Duration) -> io::Result<()> {
    use mooring_core::terminal::{agent_input_ready, command_missing};
    let started = Instant::now();
    let mut last = String::new();
    let mut still = 0;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let tail = match rpc(
            socket,
            &Request::ReadScreen {
                id: id.into(),
                rows: 40,
            },
        )? {
            Reply::Text(text) => text.trim_end().to_owned(),
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        };
        if command_missing(&tail) {
            return Err(io::Error::other("找不到这个 Agent 的命令，没有交任务"));
        }
        if tail == last {
            still += 1;
        } else {
            last = tail;
            still = 0;
        }
        if still >= 2 && agent_input_ready(&last) {
            return expect_ok(rpc(
                socket,
                &Request::SendText {
                    id: id.into(),
                    text: text.into(),
                    submit: true,
                },
            )?);
        }
        if started.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Agent 一直没有出现输入框（可能停在登录、信任目录或更新提示上），没有交任务",
            ));
        }
    }
}
/// A pane opened beside a terminal, or in its side tab.
struct Opened {
    id: String,
    tab: Option<String>,
    tab_title: String,
    beside: Option<String>,
    /// Why it went to a side tab instead of beside, when it did.
    overflow: Option<String>,
}
fn split(a: Args) -> io::Result<()> {
    let opened = open(&a, a.arg(0, "终端")?, &a.rest, a.has("tab"))?;
    if a.has("json") {
        println!(
            "{}",
            serde_json::json!({
                "id": opened.id,
                "tab": opened.tab,
                "tab_title": opened.tab_title,
                "beside": opened.beside,
                "overflow": opened.overflow,
            })
        );
    } else {
        println!("{}", opened.id);
        if let Some(why) = &opened.overflow {
            // On stderr, so `id=$(mooring split …)` still captures only the id.
            eprintln!(
                "{why}，开在了「{}」标签（不切换用户的画面）",
                opened.tab_title
            );
        }
    }
    Ok(())
}
/// Opens a shell beside `needle`'s terminal (or in its side tab when `to_tab`, when that
/// tab is full, or when a split would leave a half too small) running `command`.
fn open(a: &Args, needle: &str, command: &[String], to_tab: bool) -> io::Result<Opened> {
    if a.has("here") && to_tab {
        return Err(io::Error::other("--here 和 --tab 只能选一个"));
    }
    let w = workspace(&a.socket)?;
    let target = resolve(&w, needle)?;
    let source = w
        .terminals
        .iter()
        .find(|t| t.id == target)
        .ok_or_else(|| io::Error::other("终端不存在"))?;
    let tab = w
        .terminal_tab(&target)
        .ok_or_else(|| io::Error::other("这个终端没有摆在任何画面里，不能在它旁边分屏"))?;
    let share = match a.value("size") {
        None => 5000,
        Some(v) => {
            let f: f64 = v
                .parse()
                .map_err(|_| io::Error::other("--size 需要 0 到 1 之间的小数"))?;
            if !(f > 0.0 && f < 1.0) {
                return Err(io::Error::other("--size 需要 0 到 1 之间的小数"));
            }
            ((f * 10000.0).round() as u16).clamp(1, 9999)
        }
    };
    let axis = if a.has("down") {
        Axis::Down
    } else {
        Axis::Right
    };
    // Asked before anything starts, so a refused split leaves nothing behind; a pane
    // beside another starts at the size it will show at.
    let panes = w.tab_terminals(&tab.id).len();
    let (placement, why) = if to_tab {
        (None, "按 --tab".to_owned())
    } else if panes >= MAX_PANES {
        let why = format!("这个标签已经有 {panes} 块");
        if a.has("here") {
            return Err(io::Error::other(format!(
                "{why}，不再往里加；去掉 --here 会开在旁屏标签"
            )));
        }
        (None, why)
    } else {
        match split_room(&a.socket, &target, axis, share) {
            Ok((cols, rows)) => (
                Some(Placement::Beside {
                    target: target.clone(),
                    axis,
                    share,
                    cols,
                    rows,
                }),
                String::new(),
            ),
            Err(e) if a.has("here") => return Err(e),
            Err(e) => (None, e.to_string()),
        }
    };
    let overflow = placement.is_none();
    let placement = match placement {
        Some(p) => p,
        None => side_placement(&a.socket, &w, &side_owner(&w, tab, &target), &target)?,
    };
    let cwd = match a.value("cwd") {
        Some(dir) => std::fs::canonicalize(dir)
            .map_err(|e| io::Error::other(format!("--cwd {dir}：{e}")))?
            .to_string_lossy()
            .into_owned(),
        // Where that shell is now, as the workbench's own split does.
        None => match rpc(&a.socket, &Request::TerminalCwd { id: target.clone() }) {
            Ok(Reply::Cwd(dir)) if Path::new(&dir).is_dir() => dir,
            _ => source.cwd.clone(),
        },
    };
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into());
    let argv = split_argv(&shell, command);
    let id = format!(
        "split-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    // A new tab's pane is sized by the workbench once that tab is shown.
    let (cols, rows) = match &placement {
        Placement::Beside { cols, rows, .. } => (*cols, *rows),
        Placement::NewTab { .. } => (100, 30),
    };
    match rpc(
        &a.socket,
        &Request::Launch {
            id: id.clone(),
            space: source.space.clone(),
            argv,
            cwd,
            cols,
            rows,
            scheme: Default::default(),
        },
    )? {
        Reply::Workspace(_) => (),
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
    let (edit, beside) = match placement {
        Placement::Beside {
            target,
            axis,
            share,
            ..
        } => (
            Edit::SplitPane {
                target: target.clone(),
                terminal: id.clone(),
                axis,
                before: false,
                share,
            },
            Some(target),
        ),
        Placement::NewTab { id: tab, title } => (
            Edit::PutTab {
                tab: Tab {
                    id: tab,
                    space: source.space.clone(),
                    default_task: None,
                    title,
                    layout: Some(Layout::Leaf(id.clone())),
                },
            },
            None,
        ),
    };
    if let Err(e) = rpc(&a.socket, &Request::Edit { revision: 0, edit }) {
        // Nothing shows it, so nothing would ever stop it.
        let _ = rpc(&a.socket, &Request::Stop { id });
        return Err(e);
    }
    let after = workspace(&a.socket)?;
    let placed = after.terminal_tab(&id);
    Ok(Opened {
        tab: placed.map(|t| t.id.clone()),
        tab_title: after.terminal_tab_title(&id),
        id,
        beside,
        overflow: overflow.then_some(why),
    })
}
/// A login shell, or one that runs `command` first and stays open after it, the way a
/// resumed conversation starts, so its output stays readable and the pane stays usable.
fn split_argv(shell: &str, command: &[String]) -> Vec<String> {
    let command = match command {
        [] => return vec![shell.into(), "-l".into()],
        // One argument is a command line already, `&&` and pipes included.
        [line] => line.clone(),
        words => words
            .iter()
            .map(|w| shell_quote(w))
            .collect::<Vec<_>>()
            .join(" "),
    };
    let then = format!("{command}; exec {} -l", shell_quote(shell));
    vec![shell.into(), "-l".into(), "-i".into(), "-c".into(), then]
}
/// Closes a pane as its × does: the tab goes with its last pane, the program ends, and
/// `events` reports `terminal_exited`.
fn close(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let needle = a.arg(0, "终端")?;
    // An exited pane still shows; close it by its full id.
    let id = match w.terminal_tab(needle) {
        Some(_) => needle.to_owned(),
        None => resolve(&w, needle)?,
    };
    if w.terminal_tab(&id).is_none() {
        return Err(io::Error::other("这个终端没有摆在任何画面里"));
    }
    match rpc(
        &a.socket,
        &Request::Edit {
            revision: 0,
            edit: Edit::ClosePane { terminal: id },
        },
    )? {
        Reply::Workspace(_) => Ok(()),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
/// `at` in whole seconds since the epoch, the unit content cards record `updated` in.
fn epoch_secs(at: Instant) -> u64 {
    std::time::SystemTime::now()
        .checked_sub(at.elapsed())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooring_core::agent::Agent;
    fn fixture() -> Workspace {
        let mut w = Workspace::default();
        for (id, state) in [
            ("aaaa1111", ProcessState::Running),
            ("aaaa2222", ProcessState::Running),
            ("bbbb", ProcessState::Exited),
        ] {
            w.terminals.push(Terminal {
                id: id.into(),
                space: "s".into(),
                argv: vec![],
                cwd: "/tmp".into(),
                pid: None,
                state,
                exit_code: None,
                error: None,
            });
        }
        w.tabs.push(Tab {
            id: "t".into(),
            space: "s".into(),
            title: "review".into(),
            layout: Some(Layout::Leaf("aaaa2222".into())),
            default_task: None,
        });
        w.agent_sessions.push(AgentSession {
            id: "s1".into(),
            runtime: Agent::all()[0],
            cli_session_id: "c1".into(),
            terminal: Some("aaaa1111".into()),
            current: true,
            task: None,
            outcome: Outcome::Open,
            updated: 1,
            cwd: "/tmp".into(),
            transcript: "".into(),
            name: String::new(),
            binding: None,
        });
        w
    }
    #[test]
    fn terminals_resolve_by_unique_prefix_or_tab_title_among_running_ones() {
        let w = fixture();
        assert_eq!(resolve(&w, "aaaa1").unwrap(), "aaaa1111");
        assert_eq!(resolve(&w, "review").unwrap(), "aaaa2222");
        assert!(resolve(&w, "aaaa").is_err(), "ambiguous");
        assert!(resolve(&w, "bbbb").is_err(), "exited");
    }
    #[test]
    fn the_skill_names_the_marker_and_every_command() {
        assert!(SKILL.contains("MOORING_ENV"));
        // Every command and every flag it takes, so the skill cannot fall behind the table.
        let named = flags_in(SKILL, "");
        for c in COMMANDS {
            assert!(SKILL.contains(&format!("mooring {}", c.name)), "{}", c.name);
            for f in declared(c) {
                assert!(named.contains(&f), "{} --{f}", c.name);
            }
        }
    }
    /// `--name` words in a text, without `--runtime` (every command takes it) and the
    /// command's own name (`--skill`).
    fn flags_in(text: &str, own: &str) -> std::collections::BTreeSet<String> {
        text.split("--")
            .skip(1)
            .map(|s| {
                s.chars()
                    .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                    .collect::<String>()
            })
            .filter(|f| !f.is_empty() && f != "runtime" && format!("--{f}") != own)
            .collect()
    }
    fn declared(c: &Command) -> std::collections::BTreeSet<String> {
        c.valued
            .iter()
            .chain(c.switches)
            .map(|f| f.to_string())
            .collect()
    }
    #[test]
    fn every_command_documents_exactly_the_flags_it_takes() {
        for c in COMMANDS {
            assert_eq!(flags_in(c.usage, c.name), declared(c), "{}", c.name);
            for line in c.usage.lines().filter(|l| !l.starts_with(' ')) {
                assert!(line.starts_with(&format!("mooring {}", c.name)), "{line}");
            }
            assert!(usage().contains(c.usage), "{}", c.name);
        }
        // `mooring plugins` prints its own fuller text on errors; it names the same flags.
        let plugins = command("plugins").unwrap();
        let mut own = flags_in(crate::plugins_cmd::USAGE, "--skill");
        own.remove("help");
        assert_eq!(own, declared(plugins));
        for name in DRIVES {
            assert!(
                command(name).is_some(),
                "{name} is dispatched but has no usage"
            );
        }
    }
    #[test]
    fn every_documented_flag_is_accepted_and_nothing_else() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for c in COMMANDS {
            for f in c.valued {
                let given = args(&["x", &format!("--{f}"), "1"]);
                assert!(!check(c, &given).unwrap(), "{} --{f}", c.name);
                let e = check(c, &args(&["x", &format!("--{f}")])).unwrap_err();
                assert!(e.to_string().contains("需要一个值"), "{e}");
            }
            for f in c.switches {
                let given = args(&["x", &format!("--{f}"), "--runtime", "/r"]);
                assert!(!check(c, &given).unwrap(), "{} --{f}", c.name);
            }
            let e = check(c, &args(&["x", "--timout", "60"])).unwrap_err();
            assert!(e.to_string().contains("--timout"), "{e}");
            assert!(e.to_string().contains(c.usage), "{e}");
            // Help anywhere wins over everything, so the command never acts.
            assert!(check(c, &args(&["x", "--help"])).unwrap(), "{}", c.name);
            assert!(check(c, &args(&["-h", "--timout"])).unwrap(), "{}", c.name);
        }
        let split = command("split").unwrap();
        // After a bare `--` the words are the command's, its `--help` included.
        assert!(!check(split, &args(&["x", "--", "ls", "--help", "--bogus"])).unwrap());
        assert!(Args::parse(
            args(&["x", "--wiat"]).into_iter(),
            command("prompt").unwrap()
        )
        .is_err());
        let a = Args::parse(
            args(&["x", "--wait", "--timeout", "5"]).into_iter(),
            command("prompt").unwrap(),
        )
        .unwrap();
        assert!(a.has("wait") && a.value("timeout") == Some("5"));
    }
    #[test]
    fn standard_input_past_the_limit_is_an_error_not_a_cut() {
        assert_eq!(read_limited(&b"abcd"[..], 4).unwrap(), "abcd");
        let e = read_limited(&b"abcde"[..], 4).unwrap_err();
        assert!(e.to_string().contains("超过"), "{e}");
        assert!(read_limited(&[0xff, 0xfe][..], 4).is_err());
    }
    #[test]
    fn a_split_runs_its_command_in_a_shell_that_stays_open() {
        assert_eq!(split_argv("/bin/zsh", &[]), ["/bin/zsh", "-l"]);
        let one = split_argv("/bin/zsh", &["npm test && echo ok".into()]);
        assert_eq!(one[4], "npm test && echo ok; exec '/bin/zsh' -l");
        let words = split_argv("/bin/zsh", &["echo".into(), "a b".into()]);
        assert_eq!(&words[..4], ["/bin/zsh", "-l", "-i", "-c"]);
        assert_eq!(words[4], "'echo' 'a b'; exec '/bin/zsh' -l");
    }
    #[test]
    fn everything_after_a_bare_double_dash_is_the_command() {
        let a = Args::parse(
            ["t", "--down", "--size", "0.3", "--", "ls", "--all"]
                .into_iter()
                .map(String::from),
            command("split").unwrap(),
        )
        .unwrap();
        assert_eq!(a.positional, ["t"]);
        assert!(a.has("down"));
        assert_eq!(a.value("size"), Some("0.3"));
        assert_eq!(a.rest, ["ls", "--all"]);
        assert!(!a.has("all"));
    }
    /// `mooring list --json` says what the window says, on every sample case.
    #[test]
    fn list_reads_every_field_from_its_owner() {
        let w = mooring_core::sample::every_state();
        let at = mooring_core::content::now();
        let rows = list_json(&w);
        assert!(!rows.is_empty());
        for row in rows {
            let t = row["id"].as_str().unwrap();
            let Some(s) = w.current_agent(t) else {
                continue;
            };
            let st = status::of(&w, s, at);
            assert_eq!(row["status"], st.key(), "{t}");
            assert_eq!(row["status_label"], st.label(), "{t}");
            assert_eq!(row["phase"], st.phase().key(), "{t}");
            assert_eq!(row["title"], w.session_title(s), "{t}");
            assert_eq!(row["tab"], w.terminal_tab_title(t), "{t}");
            let term = w.terminals.iter().find(|x| x.id == t).unwrap();
            assert_eq!(row["cwd"], w.terminal_cwd(term), "{t}");
        }
    }
    #[test]
    fn phase_follows_the_current_conversation() {
        let mut w = fixture();
        assert_eq!(phase(&w, "aaaa2222"), Phase::None);
        assert_eq!(phase(&w, "aaaa1111"), Phase::Idle);
        use mooring_core::content::Turn;
        let mut c = mooring_core::content::Content {
            turn: Turn::Waiting,
            ..Default::default()
        };
        w.content.insert("s1".into(), c.clone());
        assert_eq!(phase(&w, "aaaa1111"), Phase::Waiting);
        c.turn = Turn::Running;
        w.content.insert("s1".into(), c.clone());
        assert_eq!(phase(&w, "aaaa1111"), Phase::Working);
        // A failed turn reads as the window reads it, not as a plain idle.
        c.turn = Turn::Error;
        w.content.insert("s1".into(), c);
        assert_eq!(phase(&w, "aaaa1111"), Phase::Idle);
        assert_eq!(status(&w, "aaaa1111"), Some(Status::Error));
    }
}
