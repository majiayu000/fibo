//! Read-only CLI identity discovery. A shared cwd is never association evidence.
use mooring_core::model::{Agent, AgentSession, Binding, Outcome, ProcessState, Workspace};
use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
};

use std::sync::atomic::{AtomicU64, Ordering};
static PS_SPAWNS: AtomicU64 = AtomicU64::new(0);
pub(crate) static LSOF_SPAWNS: AtomicU64 = AtomicU64::new(0);
pub fn probe_counts() -> (u64, u64) {
    (
        PS_SPAWNS.load(Ordering::Relaxed),
        LSOF_SPAWNS.load(Ordering::Relaxed),
    )
}

#[derive(Clone, Debug)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub runtime: Option<Agent>,
    pub cli_session_id: Option<String>,
    /// Codex started with `--no-daemon`: its own process opens the log, so lsof will bind it.
    pub no_daemon: bool,
    /// The flags a resume of its conversation repeats; empty for other processes.
    pub launch: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct Transcript {
    pub runtime: Agent,
    pub cli_id: String,
    pub path: String,
    pub cwd: String,
    pub updated: u64,
}

mod processes;
pub use processes::{claude_transcript_off, processes};

pub fn roots() -> io::Result<Vec<(Agent, PathBuf)>> {
    let home =
        mooring_platform::dirs::home().ok_or_else(|| io::Error::other("HOME unavailable"))?;
    Ok(crate::agents::roots(&home))
}
/// The CLI whose transcript root holds `path` (the deepest root, should one nest in
/// another), or None when it lies under none. Other CLIs read Claude's settings and run
/// its hooks (Grok does), so a hook's `--agent` alone does not say whose transcript it names.
pub fn transcript_owner(path: &Path, roots: &[(Agent, PathBuf)]) -> Option<Agent> {
    roots
        .iter()
        .filter_map(|(agent, root)| {
            let root =
                mooring_platform::file_watch::normalized(root).unwrap_or_else(|_| root.clone());
            (path.starts_with(&root) && crate::agents::adapter(*agent).is_transcript(path))
                .then_some((root.components().count(), *agent))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, agent)| agent)
}
fn ancestors(pid: u32, processes: &[Process]) -> Vec<&Process> {
    let by_id: HashMap<_, _> = processes.iter().map(|p| (p.pid, p)).collect();
    let mut seen = HashSet::new();
    let mut pid = pid;
    let mut out = vec![];
    while seen.insert(pid) {
        let Some(process) = by_id.get(&pid) else {
            break;
        };
        out.push(*process);
        pid = process.parent;
    }
    out
}
/// Whether an Agent was started from another one's shell, not merely wrapped by it: a
/// node wrapper is the native CLI's direct parent, a shell tool puts a shell between them.
fn nested(process: &Process, processes: &[Process]) -> bool {
    let mut crossed = false;
    for ancestor in ancestors(process.parent, processes) {
        match ancestor.runtime {
            Some(_) if crossed => return true,
            Some(_) => (),
            None => crossed = true,
        }
    }
    false
}
pub fn terminal_for_process(
    pid: u32,
    processes: &[Process],
    workspace: &Workspace,
) -> Option<String> {
    for process in ancestors(pid, processes) {
        if let Some(t) = workspace
            .terminals
            .iter()
            .find(|t| t.pid == Some(process.pid) && t.state == ProcessState::Running)
        {
            return Some(t.id.clone());
        }
    }
    None
}
/// A conversation only its screen tied to a pane has no other evidence of its process: a host
/// that restarted finds no id on the command line, no open log and no hook. The one CLI
/// process in that pane is it. If it runs another conversation by now, the screen moves
/// the pane there as it would have, which takes knowing this process.
pub fn only_cli_of_screen_bound(
    session: &AgentSession,
    process: &Process,
    processes: &[Process],
    workspace: &Workspace,
) -> bool {
    let terminal = session.terminal.as_deref();
    let in_pane = |p: &&Process| {
        p.runtime == process.runtime
            && !processes
                .iter()
                .any(|c| c.parent == p.pid && c.runtime == p.runtime)
            && terminal_for_process(p.pid, processes, workspace).as_deref() == terminal
    };
    let mut clis = processes.iter().filter(in_pane);
    session.binding == Some(Binding::Screen)
        && process.cli_session_id.is_none()
        && clis.next().is_some_and(|only| only.pid == process.pid)
        && clis.next().is_none()
}
/// Names the Agent processes whose command line names no conversation, from their CLI's
/// own list of live sessions (`Adapter::sessions_by_pid`): those in an unbound pane, and
/// those in a pane whose conversation has no process yet (`watched` holds the sessions
/// that do). A host that restarted has lost that link, and an Agent waiting on a background
/// shell sends no hook to bring it back. Asked once per probe per CLI, and only while such
/// a process runs in a managed pane.
pub fn name_by_pid(workspace: &Workspace, processes: &mut [Process], watched: &HashSet<String>) {
    let mut unnamed: Vec<(Agent, Vec<u32>)> = vec![];
    // Pid -> the conversation its pane is bound to.
    let mut bound_to: HashMap<u32, String> = HashMap::new();
    for p in processes.iter() {
        let Some(runtime) = p.runtime.filter(|_| p.cli_session_id.is_none()) else {
            continue;
        };
        let Some(terminal) = terminal_for_process(p.pid, processes, workspace) else {
            continue;
        };
        if let Some(s) = workspace.current_agent(&terminal) {
            if s.runtime != runtime || watched.contains(&s.id) {
                continue;
            }
            bound_to.insert(p.pid, s.cli_session_id.clone());
        }
        match unnamed.iter_mut().find(|(a, _)| *a == runtime) {
            Some((_, pids)) => pids.push(p.pid),
            None => unnamed.push((runtime, vec![p.pid])),
        }
    }
    for (runtime, pids) in unnamed {
        for (pid, id) in crate::agents::adapter(runtime).sessions_by_pid(&pids) {
            // A bound pane keeps its conversation: the list only finds the process that
            // runs it. Anything else there, like a nested `claude -p`, stays unnamed.
            if bound_to.get(&pid).is_some_and(|bound| *bound != id) {
                continue;
            }
            if let Some(p) = processes.iter_mut().find(|p| p.pid == pid) {
                p.cli_session_id = Some(id);
            }
        }
    }
}
pub(crate) fn observation(t: &Transcript, terminal: Option<String>) -> AgentSession {
    AgentSession {
        id: uuid::Uuid::new_v4().to_string(),
        runtime: t.runtime,
        cli_session_id: t.cli_id.clone(),
        current: terminal.is_some(),
        terminal,
        task: None,
        outcome: Outcome::Open,
        updated: t.updated,
        cwd: t.cwd.clone(),
        transcript: t.path.clone(),
        binding: None,
        name: String::new(),
    }
}
/// The conversation now running in `terminal`, and the evidence that says so.
pub(crate) fn bound(t: &Transcript, terminal: String, binding: Binding) -> AgentSession {
    AgentSession {
        binding: Some(binding),
        ..observation(t, Some(terminal))
    }
}
/// `named` is the terminal the hook's environment says it runs in (`MOORING_TERMINAL`).
/// The process tree decides when it can be read; the name only stands in when the hook's
/// caller already left it, and never when the two disagree.
// Each input is a separate piece of hook evidence; bundling them would hide which one decides.
#[allow(clippy::too_many_arguments)]
pub fn apply_hook(
    workspace: &mut Workspace,
    runtime: Agent,
    pid: u32,
    named: Option<&str>,
    cli_id: &str,
    transcript: &Transcript,
    processes: &[Process],
    ended: bool,
) -> io::Result<()> {
    let agent = ancestors(pid, processes)
        .into_iter()
        .find(|p| p.runtime.is_some());
    if agent.is_some_and(|a| a.runtime != Some(runtime)) {
        return Err(io::Error::other(
            "Hook runtime disagrees with process identity",
        ));
    }
    let traced = agent.and_then(|a| terminal_for_process(a.pid, processes, workspace));
    let named = named.filter(|id| {
        workspace
            .terminals
            .iter()
            .any(|t| &t.id == id && t.state == ProcessState::Running)
    });
    let binding = if traced.is_none() && named.is_none() {
        Binding::Continued
    } else {
        Binding::Hook
    };
    let terminal = match (traced, named) {
        (Some(traced), Some(named)) if traced != named => {
            return Err(io::Error::other(
                "Hook terminal disagrees with process ancestry",
            ));
        }
        (Some(traced), _) => traced,
        (None, Some(named)) => named.to_owned(),
        (None, None) => match continued_terminal(workspace, runtime, cli_id) {
            Some(t) => t,
            None => {
                return Err(io::Error::other(
                    "Hook process is not a descendant of a managed terminal",
                ));
            }
        },
    };
    if transcript.cli_id != cli_id {
        return Err(io::Error::other(
            "Hook session ID disagrees with transcript",
        ));
    }
    // An Agent run from inside the terminal's Agent (`claude -p` from its shell tool)
    // inherits the terminal's environment and fires the same hooks; it must not take the
    // terminal from the conversation that ran it. Its hook usually runs after its caller
    // left, so the tree is all there is: some Agent of this kind sits under another one.
    let other = workspace
        .current_agent(&terminal)
        .is_some_and(|s| s.runtime == runtime && s.cli_session_id != cli_id);
    let inner = match agent {
        Some(a) => nested(a, processes),
        None => processes.iter().any(|p| {
            p.runtime == Some(runtime)
                && nested(p, processes)
                && terminal_for_process(p.pid, processes, workspace).as_deref()
                    == Some(terminal.as_str())
        }),
    };
    if other && inner {
        return Err(io::Error::other(
            "Hook from an Agent nested in the terminal's Agent keeps the binding",
        ));
    }
    let mut observed = bound(transcript, terminal, binding);
    observed.current = !ended;
    workspace.observe_agent(observed).map_err(io::Error::other)
}
/// Claude Code can hand a terminal's conversation to its background daemon: a forked
/// session goes on outside the terminal's process tree, the old log ends with
/// `continued-in`, and the terminal's CLI only shows it. The terminal whose conversation
/// it continues, still showing that one or already this one, holds it.
pub fn continued_terminal(workspace: &Workspace, runtime: Agent, cli_id: &str) -> Option<String> {
    workspace
        .agent_sessions
        .iter()
        .filter(|s| {
            s.runtime == runtime
                && workspace
                    .content
                    .get(&s.id)
                    .is_some_and(|c| c.continued_in == cli_id)
        })
        .filter_map(|s| s.terminal.as_deref())
        .find(|t| {
            workspace
                .terminals
                .iter()
                .any(|x| x.id == *t && x.state == ProcessState::Running)
                && workspace.current_agent(t).is_some_and(|c| {
                    c.runtime == runtime
                        && (c.cli_session_id == cli_id
                            || workspace
                                .content
                                .get(&c.id)
                                .is_some_and(|x| x.continued_in == cli_id))
                })
        })
        .map(str::to_owned)
}
/// Moves each terminal whose conversation went on in another session (its log says
/// `continued-in`) to that session, once its transcript is known. The new session keeps
/// the todo the old one worked on.
pub fn follow_continued(workspace: &mut Workspace, transcripts: &[Transcript]) -> io::Result<()> {
    loop {
        let next = workspace.agent_sessions.iter().find_map(|s| {
            let to = &workspace.content.get(&s.id)?.continued_in;
            let terminal = s.terminal.clone().filter(|_| s.current && !to.is_empty())?;
            let t = transcripts.iter().find(|t| {
                t.runtime == s.runtime && &t.cli_id == to && t.cli_id != s.cli_session_id
            })?;
            Some((terminal, t.clone(), s.task.clone()))
        });
        let Some((terminal, transcript, task)) = next else {
            return Ok(());
        };
        workspace
            .observe_agent(bound(&transcript, terminal, Binding::Continued))
            .map_err(io::Error::other)?;
        if let Some(s) = workspace
            .agent_sessions
            .iter_mut()
            .find(|s| s.runtime == transcript.runtime && s.cli_session_id == transcript.cli_id)
            .filter(|s| s.task.is_none())
        {
            s.task = task;
        }
    }
}
/// Remember how each managed terminal's current conversation was started, while its
/// process is still there to ask: a host back from a crash or reboot resumes with the same
/// flags. A node wrapper and its native child carry the same ones; the first counts.
pub fn record_launch(workspace: &mut Workspace, processes: &[Process]) {
    let mut seen = HashMap::new();
    for process in processes {
        let Some(runtime) = process.runtime else {
            continue;
        };
        let Some(terminal) = terminal_for_process(process.pid, processes, workspace) else {
            continue;
        };
        if let Some(session) = workspace
            .agent_sessions
            .iter()
            .find(|s| s.current && s.runtime == runtime && s.terminal.as_ref() == Some(&terminal))
        {
            seen.entry(session.id.clone())
                .or_insert_with(|| process.launch.clone());
        }
    }
    for (id, flags) in seen {
        if flags.is_empty() {
            workspace.agent_launch.remove(&id);
        } else {
            workspace.agent_launch.insert(id, flags);
        }
    }
}
/// Refresh existing conversations, then bind only a single matching open transcript per live Agent.
/// Unassociated historical files stay in the SQLite index, outside the live workspace payload.
/// Ambiguous open-file evidence remains unbound rather than choosing by directory.
pub fn reconcile(
    workspace: &mut Workspace,
    transcripts: &[Transcript],
    processes: &[Process],
    open_files: &HashMap<u32, Vec<String>>,
) -> io::Result<()> {
    for transcript in transcripts
        .iter()
        .filter(|t| {
            workspace
                .agent_sessions
                .iter()
                .any(|s| s.runtime == t.runtime && s.cli_session_id == t.cli_id)
        })
        .cloned()
        .collect::<Vec<_>>()
    {
        workspace
            .observe_agent(observation(&transcript, None))
            .map_err(io::Error::other)?;
    }
    let mut bindings = Vec::new();
    for process in processes.iter().filter(|p| p.runtime.is_some()) {
        let Some(terminal) = terminal_for_process(process.pid, processes, workspace) else {
            continue;
        };
        let matching: Vec<_> = transcripts
            .iter()
            .filter(|t| {
                Some(t.runtime) == process.runtime
                    && match &process.cli_session_id {
                        Some(id) => id == &t.cli_id,
                        _ => open_files
                            .get(&process.pid)
                            .is_some_and(|paths| paths.contains(&t.path)),
                    }
            })
            .collect();
        // A node wrapper and its native Codex child carry the same argv: one binding, not two.
        let binding = match process.cli_session_id {
            Some(_) => Binding::Process,
            None => Binding::OpenFile,
        };
        if matching.len() == 1
            && !bindings
                .iter()
                .any(|(id, t, _): &(String, &Transcript, Binding)| {
                    id == &terminal && t.path == matching[0].path
                })
        {
            bindings.push((terminal, matching[0], binding));
        }
    }
    for (terminal, transcript, binding) in &bindings {
        if *binding == Binding::Process && hook_spoke(workspace, terminal, &transcript.cli_id) {
            continue;
        }
        // The terminal's CLI may still hold the log it handed off; that session is over.
        if handed_off(workspace, transcript) {
            continue;
        }
        if bindings.iter().filter(|(id, ..)| id == terminal).count() != 1
            || bindings
                .iter()
                .filter(|(_, t, _)| {
                    t.runtime == transcript.runtime && t.cli_id == transcript.cli_id
                })
                .count()
                != 1
        {
            continue;
        }
        workspace
            .observe_agent(bound(transcript, terminal.clone(), *binding))
            .map_err(io::Error::other)?;
    }
    follow_continued(workspace, transcripts)
}
fn handed_off(workspace: &Workspace, t: &Transcript) -> bool {
    workspace
        .agent_sessions
        .iter()
        .find(|s| s.runtime == t.runtime && s.cli_session_id == t.cli_id)
        .and_then(|s| workspace.content.get(&s.id))
        .is_some_and(|c| !c.continued_in.is_empty())
}
/// An id on argv is fixed for the life of the process; the CLI's own hook follows the
/// conversation. Once the hook has named what runs in this terminal (`/clear` or `/resume`
/// switched it), or said this very conversation ended there, argv no longer decides.
fn hook_spoke(workspace: &Workspace, terminal: &str, cli_id: &str) -> bool {
    workspace.agent_sessions.iter().any(|s| {
        s.terminal.as_deref() == Some(terminal)
            && s.binding == Some(Binding::Hook)
            && (s.current || s.cli_session_id == cli_id)
    })
}
/// A CLI with no process, file or hook evidence (shared-daemon Codex, a hand-started
/// Claude without hooks) is left with the thread's newest prompt shown verbatim on one
/// managed screen. Short prompts are refused:
/// "继续" or "ok" appear everywhere.
pub fn screen_key(prompt: &str) -> Option<String> {
    let key: String = prompt
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(160)
        .collect();
    (key.chars().count() >= 12).then_some(key)
}
pub fn screen_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
/// The question an agent is visibly blocked on, read from the bottom of a quiet screen
/// with the bundled default rules: a strict approval or choice shape, anything else is no
/// evidence, never a guess. The host reads each CLI with its own rules through
/// `screen_rules::explain`, which also says which rule decided and why.
pub fn screen_wait(tail: &str) -> Option<String> {
    crate::screen_rules::explain(crate::screen_rules::default_rules(), tail).into_question()
}
pub struct ScreenThread<'a> {
    pub transcript: &'a Transcript,
    /// The newest prompt as screen evidence; None when it is too short to tell apart.
    pub key: Option<String>,
    /// When that prompt was logged, seconds since the epoch.
    pub at: Option<u64>,
    /// When the turn that prompt opened began, if the log says. Shared-daemon Codex logs
    /// a thread's first prompt only after building the thread (5 s and more after Enter),
    /// but starts the turn at Enter.
    pub began: Option<u64>,
    /// The log's cwd with symlinks resolved, comparable with the kernel's process cwd.
    pub cwd: String,
}
pub struct ScreenTerminal {
    pub terminal: String,
    /// The CLI running there: only its own threads can bind to it.
    pub runtime: Agent,
    pub cwd: Option<String>,
    /// Whitespace-free tail of this terminal's own screen.
    pub screen: String,
    /// The thread this TUI showed before (`/new` hands the pane to a newer thread).
    pub current: Option<(String, u64, Option<String>)>,
    /// When Enter was pressed here lately, seconds since the epoch.
    pub submits: Vec<u64>,
}
/// How far apart Enter in a pane and the prompt's log time may be and still be one event.
pub const SUBMIT_WINDOW: u64 = 3;
/// Whether a thread's newest prompt is worth comparing with screens: it is recent or its
/// pane just ended. A bound one only if a screen alone tied it there and a pane of its CLI
/// has no conversation, so prompts in fully bound panes read no screens.
pub fn screen_candidate(
    recent: bool,
    released: bool,
    holder: Option<&AgentSession>,
    open_pane: bool,
) -> bool {
    (recent || released)
        && holder.is_none_or(|s| s.binding.is_none_or(|b| b == Binding::Screen) && open_pane)
}
/// Whether this screen and these Enter presses say the thread's newest prompt came from there.
pub fn prompt_evidence(thread: &ScreenThread, screen: &str, submits: &[u64]) -> bool {
    let shown = thread.key.as_ref().is_some_and(|key| screen.contains(key));
    shown
        || [thread.at, thread.began]
            .into_iter()
            .flatten()
            .any(|at| submits.iter().any(|s| s.abs_diff(at) <= SUBMIT_WINDOW))
}
/// Pair threads with terminals one-to-one. Evidence is the prompt shown verbatim on the
/// screen, or Enter pressed in that pane when the prompt was logged or its turn began (a
/// short or pasted prompt never shows verbatim). A thread fitting two panes, or a pane fitting two
/// threads, binds nothing; the next distinct prompt can settle it.
pub fn screen_bindings<'a>(
    threads: &[ScreenThread<'a>],
    terminals: &[ScreenTerminal],
) -> Vec<(String, &'a Transcript)> {
    let fits = |thread: &ScreenThread, terminal: &ScreenTerminal| {
        terminal.runtime == thread.transcript.runtime
            && terminal.cwd.as_ref().is_none_or(|cwd| cwd == &thread.cwd)
            && prompt_evidence(thread, &terminal.screen, &terminal.submits)
            && terminal.current.as_ref().is_none_or(|(id, updated, key)| {
                id != &thread.transcript.cli_id
                    && thread.transcript.updated >= *updated
                    && (thread.key.is_none() || key.as_ref() != thread.key.as_ref())
            })
    };
    let mut out = vec![];
    for thread in threads {
        let mut matched = terminals.iter().filter(|t| fits(thread, t));
        let (Some(terminal), None) = (matched.next(), matched.next()) else {
            continue;
        };
        if threads.iter().filter(|other| fits(other, terminal)).count() == 1 {
            out.push((terminal.terminal.clone(), thread.transcript));
        }
    }
    out
}
pub fn open_transcripts(
    workspace: &Workspace,
    processes: &[Process],
) -> io::Result<HashMap<u32, Vec<String>>> {
    let pids = processes
        .iter()
        .filter(|p| {
            p.runtime.is_some()
                && p.cli_session_id.is_none()
                && terminal_for_process(p.pid, processes, workspace)
                    .is_some_and(|id| workspace.current_agent(&id).is_none())
        })
        .map(|p| p.pid)
        .collect::<Vec<u32>>();
    if pids.is_empty() {
        return Ok(HashMap::new());
    }
    LSOF_SPAWNS.fetch_add(1, Ordering::Relaxed);
    let mut files = mooring_platform::inspect::open_files(&pids)?;
    for open in files.values_mut() {
        open.retain(|path| path.ends_with(".jsonl"));
    }
    files.retain(|_, open| !open.is_empty());
    Ok(files)
}

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod tests;
