//! What the host started a terminal's Agent with (`Workspace::agent_starts`), as evidence
//! of the conversation that terminal runs. The host owns this fact: it put the first
//! message on the command line, or the conversation to resume. The screen does not have
//! it verbatim when the CLI draws a prompt its own way: OpenCode puts `┃` before every
//! line, wrapped ones too, and Codex renders `code` without its backticks (RC3 walk,
//! 2026-11-17). A first message passed on the command line has no Enter press either.
use super::{bound, terminal_for_process, Process, ScreenTerminal, ScreenThread, Transcript};
use fibo_core::model::{Agent, AgentStart, Binding, Workspace};

/// A start as the screen comparison holds it: the user's words of its first message with no
/// whitespace, when the host started it, and where (symlinks resolved).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launched {
    pub text: String,
    pub at: u64,
    pub cwd: String,
}

/// What a thread began with, as a start is compared with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct First {
    /// Its first user message as `words` gives it.
    pub text: String,
    /// When that was logged, seconds since the epoch.
    pub at: u64,
    /// Still its newest prompt: nothing was sent after it.
    pub newest: bool,
    /// A terminal has run it: Fibo tied it to one, which may run another by now, or have
    /// ended. It began there, so it is no start's evidence for another pane, however that
    /// pane's own evidence has aged since (its Enter presses, kept only in memory and only the
    /// last eight). A twin started with the same task in the same folder never ran it.
    pub ran: bool,
}

impl First {
    /// What a log says a thread began with: `logged`, its first user message as the index keeps
    /// it (`content::user_text` already), logged at `at`.
    pub fn logged(logged: &str, at: u64, newest: bool, ran: bool) -> Self {
        Self {
            text: fibo_core::prompt_pin::plain(logged),
            at,
            newest,
            ran,
        }
    }
}

/// The first message a terminal's Agent was started with, for a pane running that Agent.
/// None when it was started with none, with nothing a log keeps as the user's words (a fenced
/// block alone), or with too short a message to tell it from other conversations begun there:
/// "继续" or "ok" begin many, so the same least as a prompt on the screen (`screen_key`). The
/// least counts the message as it was sent, whitespace aside: a fenced block or quoted lines
/// the log drops from its words still made it this one message. A phone's few words over a
/// pasted error are no "继续", and counted on the words alone such a task never bound (#403
/// review, round two). Both sides still compare only the words.
pub fn launched(start: &AgentStart, runtime: Agent, cwd: String) -> Option<Launched> {
    let prompt = start.prompt.as_deref().filter(|_| start.agent == runtime)?;
    let text = words(prompt);
    let sent = super::screen_text(prompt).chars().count();
    (!text.is_empty() && sent >= super::KEY_LEAST).then_some(Launched {
        text,
        at: start.at,
        cwd,
    })
}

/// A message as both sides compare it: what a log keeps as the user's words
/// (`content::user_text`, which drops quoted lines and fenced blocks, say), with no whitespace
/// and no `[Image #n]` placeholders. A picture handed over with the message is no word of it,
/// and each CLI places its placeholder its own way: Codex 0.169 logs a task started with
/// `--image` as "[Image #1]\n<task>" (a phone's 新任务 with a picture, 2026-11-13), while the
/// command line handed it the task alone.
pub fn words(message: &str) -> String {
    fibo_core::prompt_pin::plain(&crate::content::user_text(message))
}

/// The least of a long message a log must keep for its start alone to say which it was.
/// OpenCode's records keep 4000 characters of a message.
const KEPT_ENOUGH: usize = 1000;

/// Whether `logged`, the first user message of a log, is `sent`, the message an Agent was
/// started with; both as `words` gives them. The whole message, not a prefix of it, unless
/// the log keeps only the start of a long one.
pub fn same_message(logged: &str, sent: &str) -> bool {
    !logged.is_empty()
        && (logged == sent || (logged.chars().count() >= KEPT_ENOUGH && sent.starts_with(logged)))
}

/// Whether `thread` is the conversation its pane's Agent was started with: that pane runs no
/// conversation yet, no terminal has run the thread, and its first user message is the one the
/// host handed over, logged in that folder no earlier than the second it started. Another pane
/// started the same way fits the same thread, and `screen_bindings` then binds neither until
/// Enter in one of them tells them apart.
pub fn evidence(thread: &ScreenThread, terminal: &ScreenTerminal) -> bool {
    let (Some(start), Some(first)) = (&terminal.launched, &thread.first) else {
        return false;
    };
    !first.ran
        && terminal.current.is_none()
        && first.at >= start.at
        && terminal.cwd.as_deref().unwrap_or(&start.cwd) == thread.cwd
        && same_message(&first.text, &start.text)
}

/// Whether `thread`'s newest prompt is still the message `terminal`'s Agent was started with:
/// it began with that, and nothing was sent after. Enter there for it (a trust question
/// answered just as the task is logged) says nothing the start does not; Enter for anything
/// else means the pane went on past its start.
pub fn still_the_start(thread: &ScreenThread, terminal: &ScreenTerminal) -> bool {
    match (&terminal.launched, &thread.first) {
        (Some(start), Some(first)) => first.newest && same_message(&first.text, &start.text),
        _ => false,
    }
}

/// What `resume` reads of the live terminals: the ones whose CLI shows its input now
/// (`host::launch_probe::input_shown`), and a process's command line.
pub struct Live<'a> {
    pub shown: &'a std::collections::HashSet<String>,
    pub argv: &'a dyn Fn(u32) -> Vec<String>,
}

/// Ties each terminal the host started resuming a conversation to that conversation, once
/// the CLI it started is there to run it and shows its input: one native process of that CLI
/// in the terminal whose command line carries that conversation's id, as the host's resume
/// command put it there, and names no other. Nothing ties it while that conversation's log
/// is unknown or it runs in another terminal. A CLI whose command line names it (Claude's
/// `--resume`, `codex resume`) is bound by `reconcile` already; OpenCode's names none Fibo
/// reads as evidence (docs/decisions/2026-11-07-agent-regression.md), so the host's own
/// word is what ties it. Its id on the command line only says the process is the one the
/// host started: another CLI started there by hand later is not, and resumes nothing the
/// host knows of. Bound as `Binding::Screen`: a later prompt in the TUI (`/new`, another
/// session picked) can move the pane on, as for any screen-tied one.
///
/// Not before the CLI shows its input: OpenCode 1.19.5 turns on raw mode, bracketed paste
/// and the alternate screen, then loads, and drops whatever is typed until its prompt is
/// focused (2026-11-17, `rc4/fix-r1/tui-ready-*.jsonl`); tied earlier, the phone's first
/// message after 接着做 was accepted and lost. Returns the CLIs it found as the host's own,
/// tied or not, with their terminals: one that exits untied ends its start.
pub fn resume(
    workspace: &mut Workspace,
    transcripts: &[Transcript],
    processes: &[Process],
    live: &Live,
) -> Vec<(u32, String)> {
    let due: Vec<(String, AgentStart)> = workspace
        .agent_starts
        .iter()
        .filter(|(_, start)| start.resume.is_some())
        .map(|(terminal, start)| (terminal.clone(), start.clone()))
        .collect();
    let mut found = vec![];
    for (terminal, start) in due {
        let Some(id) = start.resume.as_deref() else {
            continue;
        };
        if workspace.current_agent(&terminal).is_some() {
            continue;
        }
        let carries = |p: &Process| {
            (live.argv)(p.pid)
                .iter()
                .any(|a| a == id || a.strip_suffix(id).is_some_and(|f| f.ends_with('=')))
        };
        let mut clis = processes.iter().filter(|p| {
            p.runtime == Some(start.agent)
                && !processes
                    .iter()
                    .any(|c| c.parent == p.pid && c.runtime == p.runtime)
                && terminal_for_process(p.pid, processes, workspace).as_deref()
                    == Some(terminal.as_str())
                && carries(p)
        });
        let (Some(cli), None) = (clis.next(), clis.next()) else {
            continue;
        };
        found.push((cli.pid, terminal.clone()));
        if cli
            .cli_session_id
            .as_deref()
            .is_some_and(|named| named != id)
            || !live.shown.contains(&terminal)
        {
            continue;
        }
        let mut logs = transcripts
            .iter()
            .filter(|t| t.runtime == start.agent && t.cli_id == id);
        let (Some(log), None) = (logs.next(), logs.next()) else {
            continue;
        };
        let elsewhere = workspace.agent_sessions.iter().any(|s| {
            s.current
                && s.runtime == log.runtime
                && s.cli_session_id == log.cli_id
                && s.terminal.as_deref() != Some(terminal.as_str())
        });
        if elsewhere {
            continue;
        }
        // Refused only when another path tied it meanwhile; that one stands.
        let _ = workspace.observe_agent(bound(log, terminal, Binding::Screen));
    }
    found
}
