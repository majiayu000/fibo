//! Small, sourced projections. The transcript and mechanical receipts remain distinct.
use crate::model::AgentSession;
#[cfg(test)]
use crate::model::Workspace;
pub use crate::subagents::Subagent;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub path: String,
    pub inode: u64,
    pub offset: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub text: String,
    pub at: u64,
    pub source: Source,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: String,
    pub summary: String,
    pub exit_code: Option<i32>,
    pub at: u64,
    pub source: Source,
}
impl Evidence {
    /// A record a criterion can hold: a test run, a commit or a file change. Plain commands,
    /// errors and an Agent's own words are never evidence.
    pub fn attachable(&self) -> bool {
        matches!(self.kind.as_str(), "file" | "test" | "commit")
    }
    /// It proves something: attachable and it succeeded. A failed test is kept as an attempt.
    pub fn proves(&self) -> bool {
        self.attachable() && self.exit_code == Some(0)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Turn {
    #[default]
    Unknown,
    Running,
    Waiting,
    Completed,
    Interrupted,
    Error,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub hidden_reason: String,
    #[serde(default)]
    pub first_ask_known: bool,
    pub title: String,
    /// The name the CLI itself keeps for the conversation: Claude's `/rename` title,
    /// Codex's generated thread name. Empty when it has none.
    #[serde(default)]
    pub name: String,
    pub asks: Vec<Quote>,
    pub evidence: Vec<Evidence>,
    pub last_reply: String,
    pub turn: Turn,
    pub turn_started: u64,
    pub turn_key: String,
    pub updated: u64,
    pub completed_claim: bool,
    pub waiting: String,
    pub error: String,
    pub partial_history: bool,
    pub from_offset: u64,
    pub unknown_records: u64,
    /// Work still in flight when the turn last stopped, from the Stop hook payload.
    /// `None` means no payload reported it, which is unknown rather than "nothing runs".
    #[serde(default)]
    pub background: Option<Background>,
    /// Counts entries into Waiting, so one dialog reported by several signals is one event.
    #[serde(default)]
    pub waiting_episode: u32,
    /// Which signal set the current turn: 日志, Hook, 画面 or 官方状态.
    #[serde(default)]
    pub signal: String,
    #[serde(default)]
    pub signal_at: u64,
    /// What that signal carried, one "label：text" line each: the log record, the hook
    /// event, or the screen rule and the lines it saw.
    #[serde(default)]
    pub signal_detail: String,
    /// The last quiet-screen check that found no prompt and why. Live only, never stored.
    #[serde(default)]
    pub screen_check: String,
    /// The approval question on screen now, when the screen rule `approval` takes it;
    /// what 允许一次 / 拒绝 answer. Live only, never stored.
    #[serde(default)]
    pub approval: String,
    /// A Codex subagent's parent thread id and its name, from its own session header.
    #[serde(default)]
    pub parent: String,
    #[serde(default)]
    pub agent_name: String,
    /// The account's usage limits as this conversation's log last recorded them.
    #[serde(default)]
    pub quota: Option<crate::quota::Quota>,
    /// Files this session changed, each with when it last did, newest last. Paths are as the
    /// tool call named them: absolute, or relative to the session's cwd.
    #[serde(default)]
    pub files: Vec<Touch>,
    /// The choices of the question `waiting` holds, when the agent asked through its
    /// question tool (Claude's AskUserQuestion, Codex's request_user_input): the first
    /// question's option labels in order, so a pane can offer them as buttons. Empty for
    /// approvals, screen prompts and open questions.
    #[serde(default)]
    pub choices: Vec<String>,
    /// Claude's subagents of the current turn, and older ones still running, oldest first:
    /// what the sidebar lists under the session. Read from the CLI's own files beside the
    /// transcript (`<session>/subagents/agent-*.meta.json`).
    #[serde(default)]
    pub subagents: Vec<Subagent>,
    /// Subagents a task notification in this log reported as not finished, by id.
    #[serde(default)]
    pub stopped_subagents: Vec<String>,
    /// The background shells the last Stop listed as running (Claude Code names each with
    /// its description and command). Which of them is a job the agent waits for is live
    /// evidence the host adds to its snapshot; stored, every one is `job: false`.
    #[serde(default)]
    pub shells: Vec<ShellTask>,
    /// The session this conversation went on in, when its CLI handed it to another one
    /// (Claude Code's `continued-in` record: a forked session run by its background daemon).
    /// Empty while this session is the one in use.
    #[serde(default)]
    pub continued_in: String,
}
/// A background shell the agent started and left running when its turn stopped.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellTask {
    pub id: String,
    pub description: String,
    /// Work that ends on its own and wakes the agent when it does (a test run, a build,
    /// a download): no port listening below it and younger than two hours. A server, or a
    /// shell the host could not look at, is not one.
    #[serde(default)]
    pub job: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Touch {
    pub path: String,
    pub at: u64,
    /// Lines the session's edits added to and removed from this file in the turn that last
    /// touched it, as its edit calls described them (old and new text, or a patch).
    #[serde(default)]
    pub added: u32,
    #[serde(default)]
    pub removed: u32,
}
/// Changed files a card keeps; the oldest goes first, so a long session keeps its latest.
const TOUCHES: usize = 64;
impl Content {
    /// Records that `path` changed at `at` by `added` and `removed` lines; a path already
    /// listed moves to the end, keeping its counts while the same turn touches it again.
    pub fn touch(&mut self, path: &str, at: u64, added: u32, removed: u32) {
        let (at, added, removed) = match self.files.iter().position(|t| t.path == path) {
            Some(i) => {
                let old = self.files.remove(i);
                if old.at >= self.turn_started {
                    (old.at.max(at), old.added + added, old.removed + removed)
                } else {
                    (old.at.max(at), added, removed)
                }
            }
            None => (at, added, removed),
        };
        self.files.push(Touch {
            path: path.into(),
            at,
            added,
            removed,
        });
        if self.files.len() > TOUCHES {
            self.files.remove(0);
        }
    }
}
impl Content {
    /// Lines added and removed in the current turn, over every file it touched.
    pub fn turn_lines(&self) -> (u32, u32) {
        self.files
            .iter()
            .filter(|t| t.at >= self.turn_started)
            .fold((0, 0), |(a, r), t| (a + t.added, r + t.removed))
    }
}
/// In-flight work by kind. Agents (subagents, workflows, teammates, cloud sessions, MCP
/// tasks) keep a stopped turn working. A monitor or a scheduled wakeup will wake the session
/// again, so it is not done either: it is waiting to wake, as Claude Code's agent view shows
/// it. A shell that is a job (see [`ShellTask::job`]) keeps the turn working too: it ends
/// and the agent carries on. The other shells (servers) are a mark on a finished turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Background {
    pub agents: u32,
    pub shells: u32,
    pub monitors: u32,
    pub crons: u32,
    /// Of `shells`, the jobs: set only on the host's live snapshot, never stored.
    #[serde(default)]
    pub jobs: u32,
}
impl Background {
    pub fn keeps_working(&self) -> bool {
        self.agents > 0 || self.jobs > 0
    }
    pub fn wakes(&self) -> bool {
        self.monitors > 0 || self.crons > 0
    }
    /// "后台仍有子 Agent 1 个、shell 2 个", or `None` when nothing is in flight.
    pub fn describe(&self) -> Option<String> {
        let parts: Vec<String> = [
            (self.agents, "子 Agent"),
            (self.jobs, "任务"),
            (self.shells - self.jobs.min(self.shells), "shell"),
            (self.monitors, "监视任务"),
            (self.crons, "已排定唤醒"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, kind)| format!("{kind} {n} 个"))
        .collect();
        (!parts.is_empty()).then(|| format!("后台仍有{}", parts.join("、")))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Here {
    pub ask: String,
    pub done: String,
    pub stopped: String,
    pub next: String,
}
pub fn short(text: &str, limit: usize) -> String {
    let mut s: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        s.push('…');
    }
    s
}
/// Agent prose as one readable line: Markdown emphasis, headings, quotes, list
/// bullets and table rules go; table cells read as `a · b`.
pub fn plain_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.chars().all(|c| "|-: ".contains(c)))
        .map(|l| {
            let l = l
                .trim_start_matches('#')
                .trim_start_matches('>')
                .trim_start();
            let l = ["- ", "* ", "+ "]
                .iter()
                .find_map(|b| l.strip_prefix(b))
                .unwrap_or(l);
            let l = l.replace("**", "").replace("__", "").replace('`', "");
            l.trim_matches('|')
                .split('|')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
/// A first ask as a name: the polite lead-in most asks share ("你帮我看看") goes,
/// so tabs and rows start with what differs. Runs of spaces, as in a pasted table
/// row, become one so the name is not mostly blank. Pasted page chrome, table rules
/// and bare ids name nothing and go while words remain; a link reads as its site and
/// last path segment, and a leading slash command gives way to its argument.
pub fn topic(title: &str) -> String {
    let lines: Vec<&str> = title.lines().filter(|l| !l.trim().is_empty()).collect();
    let kept: Vec<&str> = lines.iter().copied().filter(|l| !noise(l)).collect();
    let title = if kept.is_empty() { lines } else { kept }
        .iter()
        .flat_map(|l| l.split_whitespace())
        .map(link)
        .collect::<Vec<_>>()
        .join(" ");
    const LEAD: [&str; 20] = [
        "你",
        "请",
        "帮我",
        "帮忙",
        "麻烦",
        "能不能",
        "可以",
        "看看",
        "看下",
        "看一下",
        "查查",
        "查一下",
        "一下",
        "我想要",
        "我想",
        "我准备",
        "我要",
        "，",
        ",",
        " ",
    ];
    // A pasted image leaves "[Image #1]" where it was; it names nothing.
    fn image(s: &str) -> Option<(usize, usize)> {
        let at = s.find("[Image #")?;
        let n = &s[at + "[Image #".len()..];
        let digits = n.len() - n.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        n[digits..].starts_with(']').then_some(())?;
        (digits > 0).then_some((at, at + "[Image #".len() + digits + 1))
    }
    let mut rest = title.trim();
    // "/goal 只要一张立绘" names what follows the command.
    if let Some((command, argument)) = rest.split_once(' ')
        && command.len() > 1
        && command.starts_with('/')
        && !command[1..].contains('/')
        && argument.trim().chars().count() >= 4
    {
        rest = argument.trim_start();
    }
    loop {
        // An image placeholder goes whenever words follow it, however few.
        if let Some((0, to)) = image(rest)
            && !rest[to..].trim().is_empty()
        {
            rest = rest[to..].trim_start();
            continue;
        }
        match LEAD.iter().find_map(|p| rest.strip_prefix(p)) {
            Some(next) if next.trim().chars().count() >= 4 => rest = next,
            _ => break,
        }
    }
    // Placeholders further in go too, as long as words remain.
    let mut bare = rest.to_owned();
    while let Some((from, to)) = image(&bare) {
        bare.replace_range(from..to, " ");
    }
    if bare.trim().is_empty() {
        return rest.trim().to_owned();
    }
    bare.split_whitespace().collect::<Vec<_>>().join(" ")
}
/// A pasted line that names nothing: a web page's skip link, a box-drawn table row,
/// a lone markup tag, or one that starts with a bare hex id.
fn noise(line: &str) -> bool {
    let l = line.trim();
    let first = l.split_whitespace().next().unwrap_or("");
    matches!(
        l,
        "跳至主要内容" | "跳到主要内容" | "Skip to main content" | "Skip to content"
    ) || l
        .chars()
        .filter(|c| ('\u{2500}'..='\u{257f}').contains(c))
        .count()
        >= 2
        || (l.starts_with('<') && l.ends_with('>'))
        || (first.len() >= 16 && first.chars().all(|c| c.is_ascii_hexdigit()))
}
/// `https://www.github.com/a/claude-skills?x=1` reads as `github.com/claude-skills`,
/// also when words run straight into it ("报错了https://…").
fn link(word: &str) -> String {
    let Some(at) = word.find("https://").or_else(|| word.find("http://")) else {
        return word.to_owned();
    };
    let (lead, url) = word.split_at(at);
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let mut parts = rest.split('/').filter(|p| !p.is_empty());
    let Some(host) = parts.next() else {
        return word.to_owned();
    };
    let host = host.strip_prefix("www.").unwrap_or(host);
    let parts: Vec<&str> = parts.collect();
    // A trailing id names nothing: `x.com/a/status/2116…` reads as `x.com/a`. A short
    // number is kept, as an issue or pull request number: `github.com/codex#425`.
    let number = parts
        .last()
        .filter(|p| p.chars().all(|c| c.is_ascii_digit()));
    let name = parts.iter().rev().find(|p| {
        !p.chars().all(|c| c.is_ascii_digit())
            && !matches!(
                **p,
                "status"
                    | "issues"
                    | "pull"
                    | "pulls"
                    | "discussions"
                    | "commit"
                    | "post"
                    | "posts"
            )
    });
    let mut short = match name {
        Some(name) => format!("{host}/{name}"),
        None => host.to_owned(),
    };
    if let Some(n) = number.filter(|n| n.len() <= 6) {
        short.push('#');
        short.push_str(n);
    }
    format!("{lead} {short}").trim_start().to_owned()
}
/// `Here::done` when nothing confirmed has happened yet.
pub const NO_DONE: &str = "尚无已确认的修改、提交或测试成功证据";
/// Hidden reason for a session whose working directory is temporary.
pub const TEMPORARY_DIRECTORY: &str = "临时目录会话";
impl Content {
    /// One readable line on where the session stands, without the labels its state chip
    /// already says ("回合结束；Agent 自述：", "等待输入："): the question it waits on,
    /// the error, what it was asked while it runs, or its last reply.
    pub fn headline(&self) -> String {
        let text = match self.turn {
            Turn::Waiting if !self.waiting.is_empty() => self.waiting.as_str(),
            Turn::Error if !self.error.is_empty() => self.error.as_str(),
            Turn::Running => self.asks.last().map_or("", |q| q.text.as_str()),
            _ => self.last_reply.as_str(),
        };
        let line = plain_line(text);
        if line.is_empty() {
            plain_line(&self.here().stopped)
        } else {
            line
        }
    }
    /// `hidden_reason` for this session: one Mooring started in its own terminal was
    /// started by the user, so a temporary working directory alone does not hide it.
    /// Sub-agents, SDK and print-mode runs stay hidden wherever they run.
    /// When the unfinished turn began: one still running, or one that stopped while its
    /// sub-agents, a monitor or a scheduled wakeup keep it open. `None` once it settled.
    pub fn running_since(&self) -> Option<u64> {
        let open = self.turn == Turn::Running
            || self.turn == Turn::Completed
                && self
                    .background
                    .is_some_and(|b| b.keeps_working() || b.wakes());
        (open && self.turn_started > 0).then_some(self.turn_started)
    }
    /// The turn ended and only a monitor or a scheduled wakeup will resume it.
    pub fn waking(&self) -> bool {
        self.turn == Turn::Completed
            && self
                .background
                .is_some_and(|b| !b.keeps_working() && b.wakes())
    }
    pub fn hidden_for(&self, session: &AgentSession) -> Option<&str> {
        if session.managed() && self.hidden_reason == TEMPORARY_DIRECTORY {
            return None;
        }
        self.hidden_reason()
    }
    pub fn hidden_reason(&self) -> Option<&str> {
        if !self.hidden_reason.is_empty() {
            Some(&self.hidden_reason)
        } else if !self.interactive {
            Some("缺少交互入口证据")
        } else {
            None
        }
    }
    pub fn here(&self) -> Here {
        let ask = if self.asks.is_empty() {
            "历史尾段未覆盖用户原话，要求未知".into()
        } else {
            let quotes = if self.asks.len() <= 3 {
                self.asks.iter().collect::<Vec<_>>()
            } else {
                std::iter::once(&self.asks[0])
                    .chain(self.asks.iter().skip(self.asks.len() - 2))
                    .collect()
            };
            quotes
                .into_iter()
                .map(|q| short(&q.text, 240))
                .collect::<Vec<_>>()
                .join("\n追加：")
        };
        let ask = if !self.first_ask_known && !self.asks.is_empty() {
            format!("最初要求未覆盖；已读取追加：\n{ask}")
        } else {
            ask
        };
        let facts: Vec<_> = self
            .evidence
            .iter()
            .filter(|f| f.proves())
            .rev()
            .take(4)
            .map(|f| short(&f.summary, 180))
            .collect();
        let done = if facts.is_empty() {
            NO_DONE.into()
        } else {
            facts.join("\n")
        };
        let stopped = match self.turn {
            Turn::Waiting => format!("等待输入：{}", short(&self.waiting, 240)),
            Turn::Error => format!("最后回合出错：{}", short(&self.error, 240)),
            Turn::Running => "日志最后记录：回合进行中；不代表此刻进程仍在线".into(),
            Turn::Interrupted => "回合已中断，尚未验收".into(),
            Turn::Completed => match self.background.and_then(|b| b.describe()) {
                Some(work) if self.waking() => format!(
                    "等待唤醒，{work}；Agent 自述：{}",
                    short(&self.last_reply, 220)
                ),
                Some(work) => format!(
                    "回合结束，{work}；Agent 自述：{}",
                    short(&self.last_reply, 220)
                ),
                None => format!("回合结束；Agent 自述：{}", short(&self.last_reply, 260)),
            },
            Turn::Unknown => {
                if self.last_reply.is_empty() {
                    "尾段没有明确回合结束事件，当前状态未知".into()
                } else {
                    format!(
                        "最近 Agent 自述（状态未知）：{}",
                        short(&self.last_reply, 240)
                    )
                }
            }
        };
        let next = match self.turn {
            Turn::Completed if self.background.is_some_and(|b| b.agents > 0) => {
                "等后台子 Agent 结束后再核对结果"
            }
            Turn::Completed if self.background.is_some_and(|b| b.jobs > 0) => {
                "等后台任务跑完，Agent 会接着做"
            }
            Turn::Completed if self.waking() => "等它被监视任务或定时唤醒，回合结束后再核对",
            Turn::Waiting => "回应上面的审批或问题",
            Turn::Error => "检查失败证据后决定是否继续",
            Turn::Running => "等回合结束后核对结果",
            Turn::Completed if self.completed_claim => "按用户原话逐项核对机械证据，再决定是否验收",
            _ => "先核对原话与日志，再决定是否继续",
        }
        .into();
        Here {
            ask,
            done,
            stopped,
            next,
        }
    }
    /// What a running turn is doing, as a phrase to follow 正在: its latest record this
    /// turn read as words (`改 tabs.rs`, `运行 cargo build -p app`), else what it was
    /// asked (`做 修复登录`). `None` when neither says anything.
    pub fn doing(&self) -> Option<String> {
        // The turn stopped and a job it started still runs: that is what it is doing.
        if self.turn != Turn::Running {
            let jobs: Vec<&ShellTask> = self.shells.iter().filter(|t| t.job).collect();
            if let Some(first) = jobs.first() {
                let name = short(&plain_line(&first.description), DOING_CHARS);
                return Some(match (jobs.len(), name.is_empty()) {
                    (1, false) => format!("等后台任务 {name}"),
                    (n, false) => format!("等后台任务 {name} 等 {n} 个"),
                    (n, true) => format!("等后台任务 {n} 个"),
                });
            }
        }
        let this_turn = |e: &&Evidence| self.turn_started == 0 || e.at >= self.turn_started;
        let from_record =
            self.evidence
                .iter()
                .rev()
                .filter(this_turn)
                .find_map(|e| match e.kind.as_str() {
                    "file" => {
                        let path = e.summary.split_once('：').map_or(&*e.summary, |(_, p)| p);
                        let name = path.trim().rsplit('/').next().unwrap_or(path).trim();
                        (!name.is_empty()).then(|| format!("改 {name}"))
                    }
                    "test" => {
                        let command = e.summary.split(['：', '（']).next().unwrap_or_default();
                        command_gist(command).map(|c| format!("跑测试 {c}"))
                    }
                    "commit" => Some(format!("提交 {}", short(e.summary.trim(), 12))),
                    "command" => command_gist(&e.summary).map(|c| format!("运行 {c}")),
                    _ => None,
                });
        from_record.or_else(|| {
            let ask = plain_line(&self.asks.last()?.text);
            (!ask.is_empty()).then(|| format!("做 {}", short(&ask, DOING_CHARS)))
        })
    }
}
/// How long a `doing` phrase's command or ask may run before it is cut.
const DOING_CHARS: usize = 40;
/// A shell command as a short readable phrase: its program and first few words, with
/// the noise around it gone. `S=/tmp/x; cd "$S" && FOO=1 cargo build -p app 2>&1 | tail`
/// reads `cargo build -p app`. Variable assignments, `cd`, `export`, `echo` and wrappers
/// such as `env` or `time` name nothing; long paths read as their last segment.
pub fn command_gist(command: &str) -> Option<String> {
    const WRAPPERS: [&str; 7] = [
        "env",
        "time",
        "nice",
        "caffeinate",
        "sudo",
        "command",
        "exec",
    ];
    const NOISE: [&str; 12] = [
        "cd", "export", "set", "unset", "source", ".", "echo", "printf", "true", ":", "sleep",
        "pushd",
    ];
    let assignment = |w: &str| {
        w.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        })
    };
    let clean = |w: &str| {
        w.trim_matches(|c| matches!(c, '"' | '\'' | '(' | ')' | '{' | '}'))
            .trim_end_matches('…')
            .to_owned()
    };
    let segments = command
        .split(['\n', ';', '|', '&'])
        .map(str::trim)
        .filter(|s| !s.is_empty());
    for segment in segments {
        let mut words = segment
            .split_whitespace()
            .map(clean)
            .filter(|w| !w.is_empty())
            .skip_while(|w| assignment(w) || WRAPPERS.contains(&w.rsplit('/').next().unwrap_or(w)))
            .peekable();
        let Some(program) = words.next() else {
            continue;
        };
        let program = program.rsplit('/').next().unwrap_or(&program).to_owned();
        if program.is_empty()
            || NOISE.contains(&program.as_str())
            || program.starts_with('$')
            || program.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let mut gist = vec![program];
        // Redirections and heredoc markers end what the command says.
        for w in words
            .take_while(|w| !w.starts_with(['>', '<']) && !w.starts_with("2>"))
            .take(3)
        {
            gist.push(match w.rsplit_once('/') {
                Some((_, last)) if w.chars().count() > 24 && !last.is_empty() => last.to_owned(),
                _ => w,
            });
        }
        return Some(short(&gist.join(" "), DOING_CHARS));
    }
    None
}
#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;

pub fn now() -> u64 {
    crate::time::now()
}

#[cfg(test)]
#[path = "content_grouping_tests.rs"]
mod grouping_tests;
