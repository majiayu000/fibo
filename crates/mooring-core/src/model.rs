pub use crate::agent::Agent;
pub use crate::layout::{Axis, Layout};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub revision: u64,
    #[serde(default)]
    pub last_left: u64,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub outcome_history: Vec<OutcomeChange>,
    #[serde(default)]
    pub task_outcomes: std::collections::HashMap<String, Outcome>,
    /// When each todo was written down and closed, and the progress written back.
    #[serde(default)]
    pub task_logs: std::collections::HashMap<String, TaskLog>,
    /// One proven binding consumes this override; None cancels the tab default.
    #[serde(default)]
    pub pending_tasks: std::collections::HashMap<String, Option<String>>,
    /// Terminals a restarted host lost, with the conversation each was running then
    /// (None: only its shell). Terminals lost before this was kept have no entry.
    #[serde(default)]
    pub lost_agents: std::collections::HashMap<String, Option<String>>,
    /// How each conversation's Agent was last started in a Mooring terminal: the flags a
    /// resume repeats (model, permissions, config), never a prompt or an old resume.
    #[serde(default)]
    pub agent_launch: std::collections::HashMap<String, Vec<String>>,
    /// Spaces you put aside (先不管): everything in them stays and keeps running, and none
    /// of it asks for you until you take the space back.
    #[serde(default)]
    pub quiet_spaces: std::collections::BTreeSet<String>,
    /// Where each space's terminals last were when one was closed. A new tab in a space
    /// with no folder of its own starts there; a folder set on the space always wins.
    #[serde(default)]
    pub space_last_cwd: std::collections::BTreeMap<String, String>,
    pub spaces: Vec<Space>,
    pub tasks: Vec<Task>,
    pub tabs: Vec<Tab>,
    pub terminals: Vec<Terminal>,
    pub agent_sessions: Vec<AgentSession>,
    pub content: rustc_hash::FxHashMap<String, crate::content::Content>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub notifications: bool,
    /// A short sound when a session starts waiting on you or fails, with or without
    /// the system notification. Off until chosen, so an upgrade stays quiet.
    #[serde(default)]
    pub sound: bool,
    pub stale_days: u32,
    pub away_minutes: u32,
    pub default_agent: Agent,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications: false,
            sound: false,
            stale_days: 3,
            away_minutes: 30,
            default_agent: Agent::default(),
        }
    }
}
/// The deepest space whose folder holds `cwd` and lies inside `within`'s folder, when
/// there is one deeper than `within` itself: a terminal opened in a broad space (say
/// ~/code) at a project that has its own space belongs to that project's space. A
/// space without a folder, or a `cwd` outside `within`, keeps `within`.
pub fn narrower_space<'a>(spaces: &'a [Space], within: &str, cwd: &str) -> Option<&'a Space> {
    let outer = spaces.iter().find(|s| s.id == within)?;
    if outer.cwd.is_empty() {
        return None;
    }
    let cwd = std::path::Path::new(cwd);
    spaces
        .iter()
        .filter(|s| s.cwd.len() > outer.cwd.len())
        .filter(|s| std::path::Path::new(&s.cwd).starts_with(&outer.cwd) && cwd.starts_with(&s.cwd))
        .max_by_key(|s| s.cwd.len())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub cwd: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub space: String,
    pub title: String,
    pub description: String,
    pub acceptance: Vec<AcceptanceCriterion>,
    pub agent: Agent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
    /// Session records attached to this criterion, oldest first. Never an Agent's claim
    /// or an automatic acceptance flag; a failed test stays here as an attempt.
    #[serde(default, deserialize_with = "criterion_evidence")]
    pub evidence: Vec<CriterionEvidence>,
}
/// Older clients and scripts sent one source string or null here; both mean none.
fn criterion_evidence<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<CriterionEvidence>, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Array(items) => serde_json::from_value(serde_json::Value::Array(items))
            .map_err(serde::de::Error::custom),
        _ => Ok(vec![]),
    }
}
/// A copy of one session record: a session keeps only its latest few, and the criterion
/// must keep what proved it after the session moves on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionEvidence {
    pub session: String,
    pub record: crate::content::Evidence,
}
impl AcceptanceCriterion {
    /// Whether a successful test, commit or file change backs it.
    pub fn proven(&self) -> bool {
        self.evidence.iter().any(|e| e.record.proves())
    }
}
/// Criteria with the same text as an earlier one fold into it, evidence included; empty
/// ones are dropped. The order of first appearance stays.
pub fn merge_criteria(criteria: Vec<AcceptanceCriterion>) -> Vec<AcceptanceCriterion> {
    let mut merged: Vec<AcceptanceCriterion> = Vec::new();
    for mut c in criteria {
        c.description = c.description.trim().to_owned();
        if c.description.is_empty() {
            continue;
        }
        match merged.iter_mut().find(|m| m.description == c.description) {
            Some(first) => {
                for e in c.evidence {
                    if !first.evidence.contains(&e) {
                        first.evidence.push(e);
                    }
                }
            }
            None => merged.push(c),
        }
    }
    merged
}

impl Task {
    /// What goes into the Agent's own input box. Never a shell argument; blank parts
    /// leave no empty headings behind.
    pub fn prompt(&self) -> String {
        let mut parts = vec![self.title.trim().to_owned()];
        if !self.description.trim().is_empty() {
            parts.push(self.description.trim().to_owned());
        }
        let criteria: Vec<_> = self
            .acceptance
            .iter()
            .map(|c| c.description.trim())
            .filter(|c| !c.is_empty())
            .map(|c| format!("- {c}"))
            .collect();
        if !criteria.is_empty() {
            parts.push(t!("验收标准：\n{list}", list = criteria.join("\n")));
        }
        parts.join("\n\n")
    }
    /// How many criteria have evidence that proves them.
    pub fn proven(&self) -> usize {
        self.acceptance.iter().filter(|c| c.proven()).count()
    }
}
pub use mooring_platform::shell::quote_posix as shell_quote;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub space: String,
    /// A creation default, never the ownership of existing agent sessions.
    pub default_task: Option<String>,
    pub title: String,
    pub layout: Option<Layout>,
}

/// Side tabs `fibo split`/`run` opens are `side~<terminal>`, then `~2`, `~3`.
pub const SIDE_TAB: &str = "side~";
impl Tab {
    /// A side tab's title is the default it was opened with (「Codex 的旁屏」), never a
    /// name someone gave: it stays on the tab and is not handed to a conversation.
    pub fn is_side(&self) -> bool {
        self.id.starts_with(SIDE_TAB)
    }
}

/// A managed PTY. Its shell may run many distinct agent conversations over time.
/// Placement belongs exclusively to Tab.layout; PTY identity survives closing its view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terminal {
    pub id: String,
    pub space: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub pid: Option<u32>,
    pub state: ProcessState,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

/// CLI identity is (runtime, cli_session_id), independent of PTY and placement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub runtime: Agent,
    pub cli_session_id: String,
    /// Last associated terminal; retained for historical conversations.
    pub terminal: Option<String>,
    /// At most one current conversation per terminal. This is not its outcome.
    pub current: bool,
    pub task: Option<String>,
    pub outcome: Outcome,
    pub updated: u64,
    pub cwd: String,
    pub transcript: String,
    /// What the user called it; empty leaves the name to the CLI and the first ask.
    #[serde(default)]
    pub name: String,
    /// The evidence that tied it to its terminal; `None` while it has never been bound.
    #[serde(default)]
    pub binding: Option<Binding>,
}
/// How a conversation was tied to the terminal running it, strongest first. Each CLI
/// offers some of these; the host tries every one it offers and refuses ambiguity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Binding {
    /// The process names its conversation: on its command line (`--session-id`, `resume`)
    /// or in its CLI's own registry of live sessions.
    Process,
    /// The CLI's own hook reported its conversation from inside the terminal.
    Hook,
    /// The process holds the transcript open.
    OpenFile,
    /// The newest prompt shows on exactly one screen, or was submitted there.
    Screen,
    /// The conversation this terminal ran went on in this session, run by the CLI outside
    /// the terminal (Claude Code's background daemon); the terminal only shows it.
    Continued,
}
impl Binding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Process => t!("进程报出的会话 ID"),
            Self::Hook => t!("Agent 的 Hook 上报"),
            Self::OpenFile => t!("进程打开的对话文件"),
            Self::Screen => t!("画面上的提示词"),
            Self::Continued => t!("日志写明从这个终端的会话接着跑"),
        }
    }
}
/// Where a conversation's name comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameSource {
    User,
    Task,
    /// The CLI's own: Codex's generated thread name, Claude's `/rename`, desktop or
    /// generated title, Grok's summary.
    Agent,
    FirstAsk,
}
impl NameSource {
    pub fn label(self, agent: Agent) -> String {
        match self {
            Self::User => t!("你起的名").into(),
            Self::Task => t!("待办标题").into(),
            Self::Agent => t!("{name} 起的名", name = agent.name()),
            Self::FirstAsk => t!("第一句话").into(),
        }
    }
}
impl AgentSession {
    /// Started in a Mooring terminal. Conversations found only by scanning CLI logs
    /// never ask for attention; they stay visible in the overview.
    pub fn managed(&self) -> bool {
        self.terminal.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessState {
    Starting,
    Running,
    Draining,
    Exited,
    Interrupted,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    #[default]
    Open,
    Completed,
    Deferred {
        reason: String,
        until: u64,
    },
    Abandoned,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeChange {
    pub session: String,
    pub before: Outcome,
    pub after: Outcome,
    pub at: u64,
    pub reason: String,
}
impl Outcome {
    pub fn needs_review(&self, now: u64) -> bool {
        matches!(self, Self::Open) || matches!(self, Self::Deferred { until, .. } if *until <= now)
    }
}

impl Workspace {
    /// Fills an outcome edit's `seen` from this copy, the one its sender is looking at.
    pub fn stamp(&self, edit: &mut Edit) {
        match edit {
            Edit::SetOutcome { id, seen, .. } if seen.is_none() => {
                *seen = self
                    .agent_sessions
                    .iter()
                    .find(|s| &s.id == id)
                    .map(|s| SeenSession {
                        outcome: s.outcome.clone(),
                        turn: self
                            .content
                            .get(id.as_str())
                            .map(|c| c.turn_key.clone())
                            .unwrap_or_default(),
                    });
            }
            Edit::SetTaskOutcome { id, seen, .. } if seen.is_none() => {
                *seen = Some(
                    self.task_outcomes
                        .get(id.as_str())
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            _ => {}
        }
    }
    pub fn review_since(&self, id: &str) -> u64 {
        self.outcome_history
            .iter()
            .rev()
            .find(|h| h.session == id && h.reason == not_ui!("暂存到期"))
            .map(|h| h.at)
            .unwrap_or(0)
    }
    pub fn next_deadline(&self, now: u64) -> Option<u64> {
        let mut times = Vec::new();
        for session in &self.agent_sessions {
            if let Outcome::Deferred { until, .. } = session.outcome {
                times.push(until);
            }
            if let Some(c) = self.content.get(&session.id) {
                if c.hidden_for(session).is_some() || !session.outcome.needs_review(now) {
                    continue;
                }
                if c.turn == crate::content::Turn::Running && !session.current {
                    times.push(c.updated.saturating_add(901));
                }
                if c.turn != crate::content::Turn::Running || !session.current {
                    times.push(
                        (if c.turn_started > 0 {
                            c.turn_started
                        } else {
                            c.updated
                        })
                        .max(self.review_since(&session.id))
                        .saturating_add(u64::from(self.settings.stale_days) * 86400 + 1),
                    );
                }
            }
        }
        for outcome in self.task_outcomes.values() {
            if let Outcome::Deferred { until, .. } = outcome {
                times.push(*until);
            }
        }
        times.into_iter().filter(|t| *t > now).min()
    }
    pub fn task_terminal(&self, task: &str) -> Option<&Terminal> {
        let running = |t: &&Terminal| t.state == ProcessState::Running;
        self.terminals
            .iter()
            .filter(running)
            .find(|t| {
                self.current_agent(&t.id)
                    .is_some_and(|s| s.task.as_deref() == Some(task))
            })
            .or_else(|| {
                self.terminals.iter().filter(running).find(|t| {
                    self.current_agent(&t.id).is_none()
                        && self.pending_tasks.get(&t.id) != Some(&None)
                        && self
                            .terminal_tab(&t.id)
                            .is_some_and(|tab| tab.default_task.as_deref() == Some(task))
                })
            })
    }

    /// Like Warp: the name you gave it, then what runs in it (todo, conversation,
    /// the title the program set), then the program itself, numbered among same-named
    /// tabs. The space's directory is not a tab name; every tab in one space shares it.
    /// `titles` holds every terminal's program title, so a tab keeps its name while
    /// another tab is in front; `focused` breaks ties between a tab's own panes.
    pub fn tab_title(
        &self,
        tab: &Tab,
        titles: &std::collections::HashMap<String, String>,
        focused: Option<&str>,
    ) -> String {
        if let Some(title) = self.tab_given_title(tab, titles, focused) {
            return title;
        }
        let name = self.tab_program(tab);
        let same = self
            .tabs
            .iter()
            .filter(|t| t.space == tab.space)
            .take_while(|t| t.id != tab.id)
            .filter(|t| {
                self.tab_given_title(t, titles, None).is_none() && self.tab_program(t) == name
            })
            .count();
        if same == 0 {
            name
        } else {
            format!("{name} {}", same + 1)
        }
    }
    fn tab_given_title(
        &self,
        tab: &Tab,
        titles: &std::collections::HashMap<String, String>,
        focused: Option<&str>,
    ) -> Option<String> {
        let ids = self.tab_terminals(&tab.id);
        // A tab holding one conversation is called what the conversation is called,
        // everywhere the same: `session_name` is the one owner of that name.
        let mut live = self
            .agent_sessions
            .iter()
            .filter(|s| s.current && s.terminal.as_ref().is_some_and(|id| ids.contains(id)));
        if let (Some(only), None) = (live.next(), live.next())
            && let Some(name) = self.session_name(only)
        {
            return Some(name);
        }
        if !tab.title.trim().is_empty() {
            return Some(tab.title.clone());
        }
        let sessions: Vec<_> = self
            .agent_sessions
            .iter()
            .filter(|s| s.current && s.terminal.as_ref().is_some_and(|id| ids.contains(id)))
            .collect();
        if sessions.is_empty()
            && let Some(task) = tab
                .default_task
                .as_ref()
                .and_then(|id| self.tasks.iter().find(|t| &t.id == id))
        {
            return Some(task.title.clone());
        }
        if let Some(first) = sessions.first().and_then(|s| s.task.as_ref())
            && sessions.iter().all(|s| s.task.as_ref() == Some(first))
            && let Some(task) = self.tasks.iter().find(|t| &t.id == first)
        {
            return Some(task.title.clone());
        }
        if let Some(title) = sessions
            .iter()
            .filter_map(|s| Some((self.session_updated(s), self.session_name(s)?)))
            .max_by_key(|(updated, _)| *updated)
            .map(|(_, t)| t)
        {
            return Some(title);
        }
        let mut ids = ids;
        if let Some(i) = ids.iter().position(|id| Some(id.as_str()) == focused) {
            ids.swap(0, i);
        }
        ids.iter()
            .filter_map(|id| titles.get(id))
            .find(|t| !t.trim().is_empty())
            .cloned()
    }
    /// The tab name of a terminal as every surface outside the window shows it (the
    /// CLI, the phone, plugins): `tab_title` without the titles programs set, which
    /// only the window reads. Empty when the terminal sits in no tab.
    pub fn terminal_tab_title(&self, terminal: &str) -> String {
        self.terminal_tab(terminal)
            .map(|tab| self.tab_title(tab, &std::collections::HashMap::new(), None))
            .unwrap_or_default()
    }
    /// What runs in the tab's first pane: its agent, else its program ("zsh").
    fn tab_program(&self, tab: &Tab) -> String {
        let ids = self.tab_terminals(&tab.id);
        let Some(id) = ids.first() else {
            return t!("终端").into();
        };
        if let Some(session) = self.current_agent(id) {
            return session.runtime.name().into();
        }
        self.terminals
            .iter()
            .find(|t| &t.id == id)
            .and_then(|t| t.argv.first())
            .and_then(|program| program.rsplit('/').next())
            .map(|program| program.trim_start_matches('-'))
            .filter(|program| !program.is_empty())
            .unwrap_or(t!("终端"))
            .into()
    }
    pub fn tab_terminals(&self, id: &str) -> Vec<String> {
        self.tabs
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.layout.as_ref())
            .map(Layout::ids)
            .unwrap_or_default()
    }
    pub fn terminal_tab(&self, id: &str) -> Option<&Tab> {
        self.tabs
            .iter()
            .find(|t| t.layout.as_ref().is_some_and(|l| l.contains(id)))
    }
    /// What a conversation is called and where that name comes from, best first: the
    /// name the user gave it, the task it was started for or given, the one its CLI
    /// keeps, its first ask. `None` when none of those is known. Like Claude Desktop's
    /// title sources, a name the user or a task chose is never replaced by a generated one.
    pub fn session_naming(&self, s: &AgentSession) -> Option<(String, NameSource)> {
        let named = s.name.trim();
        if !named.is_empty() {
            return Some((named.to_owned(), NameSource::User));
        }
        if let Some(task) = s
            .task
            .as_ref()
            .and_then(|id| self.tasks.iter().find(|t| &t.id == id))
            .map(|t| t.title.trim())
            .filter(|t| !t.is_empty())
        {
            return Some((task.to_owned(), NameSource::Task));
        }
        let c = self.content.get(&s.id)?;
        let named = crate::content::topic(&c.name);
        if !named.is_empty() {
            return Some((crate::content::short(&named, 70), NameSource::Agent));
        }
        // The first ask keeps its lines, so pasted chrome above the words can go.
        let first = match c.asks.first() {
            Some(q) if c.first_ask_known => q.text.as_str(),
            _ => c.title.as_str(),
        };
        Some(crate::content::short(&crate::content::topic(first), 70))
            .filter(|t| !t.is_empty())
            .map(|t| (t, NameSource::FirstAsk))
    }
    /// The tab a running conversation has to itself: a name given to that tab names
    /// the conversation too, so the tab and the sidebar never disagree.
    pub fn sole_session_tab(&self, s: &AgentSession) -> Option<&Tab> {
        if !s.current {
            return None;
        }
        let tab = self.terminal_tab(s.terminal.as_deref()?)?;
        let ids = self.tab_terminals(&tab.id);
        let others = self.agent_sessions.iter().any(|o| {
            o.id != s.id && o.current && o.terminal.as_ref().is_some_and(|id| ids.contains(id))
        });
        (!others).then_some(tab)
    }
    /// When the conversation last moved: the newer of what its log index recorded and
    /// what its content (log, hook or screen) last saw. Every "N 分钟前", every sort by
    /// recency and every 今天 / 更早 split reads this.
    pub fn session_updated(&self, s: &AgentSession) -> u64 {
        self.content
            .get(&s.id)
            .map_or(s.updated, |c| c.updated.max(s.updated))
    }
    /// The folder a terminal works in as every surface names it: its conversation's
    /// when one is current there (an Agent started after a `cd` reports its own),
    /// else the one it was started in.
    pub fn terminal_cwd(&self, terminal: &Terminal) -> String {
        self.current_agent(&terminal.id)
            .map(|s| s.cwd.clone())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| terminal.cwd.clone())
    }
    /// See `status::needs_you`, the one count of 要你处理.
    pub fn needs_you(&self, s: &AgentSession, now: u64) -> bool {
        crate::status::needs_you(self, s, now)
    }
    pub fn session_name(&self, s: &AgentSession) -> Option<String> {
        self.session_naming(s).map(|(name, _)| name)
    }
    /// The name every surface shows: `session_name`, else the latest ask of a
    /// conversation read only from its tail, else its CLI and directory.
    pub fn session_title(&self, s: &AgentSession) -> String {
        if let Some(name) = self.session_name(s) {
            return name;
        }
        self.content
            .get(&s.id)
            .and_then(|c| c.asks.last())
            .map(|q| crate::content::short(&crate::content::topic(&q.text), 70))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| {
                let dir = std::path::Path::new(&s.cwd)
                    .file_name()
                    .map_or(s.cwd.clone(), |n| n.to_string_lossy().into_owned());
                format!("{} · {dir}", s.runtime.executable())
            })
    }
    pub fn current_agent(&self, terminal: &str) -> Option<&AgentSession> {
        self.agent_sessions
            .iter()
            .find(|s| s.current && s.terminal.as_deref() == Some(terminal))
    }
    /// The command that resumes a conversation with the flags its Agent was started with.
    pub fn resume_command(&self, session: &AgentSession) -> String {
        let launch = self
            .agent_launch
            .get(&session.id)
            .map_or(&[][..], Vec::as_slice);
        session
            .runtime
            .resume_command(&session.cli_session_id, launch)
    }
}

mod edit;
pub use edit::{Edit, SeenSession};
mod space;
mod task_log;
pub use task_log::{NOTE_LIMIT, TaskLog, TaskNote, short_id};
mod tabs;
#[cfg(test)]
mod tests;
mod workspace_edit;

#[cfg(test)]
mod identity_tests;
#[cfg(test)]
mod side_tab_tests;
