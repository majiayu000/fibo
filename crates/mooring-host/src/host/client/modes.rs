//! `Request::Modes`: when the Agent in a terminal may have its model or permission mode changed,
//! and what a change set. Never while it works or asks: a key there lands in whatever it asked.
//! The keys themselves are `agents/modes_drive.rs`, which knows each CLI.
use super::*;
use crate::agents::modes_drive::{self as drive, Changed, Terminal};
use mooring_core::modes::{ModeReply, ModeRequest};
use mooring_core::status;

pub(super) fn run(shared: &Shared, ask: ModeRequest) -> io::Result<Reply> {
    match ask {
        ModeRequest::Options { id, kind } => {
            let s = Steps::start(shared, &id)?;
            let options = drive::options(&s, s.agent, kind)?;
            Ok(Reply::Modes(ModeReply::Options(options)))
        }
        ModeRequest::Set {
            id,
            kind,
            choice,
            allow_full,
        } => {
            let s = Steps::start(shared, &id)?;
            let changed = drive::set(&s, s.agent, kind, &choice, allow_full)?;
            s.record(changed)
        }
    }
}

struct Steps<'a> {
    shared: &'a Shared,
    id: &'a str,
    agent: mooring_core::model::Agent,
}

impl Terminal for Steps<'_> {
    fn screen(&self) -> io::Result<String> {
        let terminal = self.terminal()?;
        let live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        live.vt.tail_text(usize::from(live.rows))
    }
    fn keys(&self, keys: &[&str]) -> io::Result<()> {
        let keys: Vec<String> = keys.iter().map(|k| (*k).to_owned()).collect();
        send_keys(self.shared, self.id, &keys)
    }
    fn paste(&self, text: &str) -> io::Result<()> {
        send_text(self.shared, self.id, text, false, None)
    }
    fn commands(&self) -> io::Result<Vec<String>> {
        let terminal = self.terminal()?;
        let shell = lock(&terminal)?.pid;
        descendants(shell)
    }
}

impl<'a> Steps<'a> {
    /// Says why not when it is working or asks, or no Agent runs here that this can change.
    fn start(shared: &'a Shared, id: &'a str) -> io::Result<Self> {
        let (agent, ok) = {
            let host = lock(shared)?;
            let w = &host.workspace;
            let session = w.current_agent(id);
            let agent = session
                .map(|s| s.runtime)
                .or_else(|| host.recognition.terminal_agents.get(id).copied())
                .ok_or_else(|| io::Error::other("这个终端里没有 Agent"))?;
            let trust = host.recognition.terminal_trust.contains_key(id);
            let ok = match session {
                _ if trust => Err("它在问要不要信任这个文件夹，答完再换"),
                Some(s) => idle(w, s),
                None => Ok(()),
            };
            (agent, ok)
        };
        if !drive::switches(agent) {
            return Err(io::Error::other("这个 Agent 还不支持在手机上换"));
        }
        ok.map_err(io::Error::other)?;
        Ok(Self { shared, id, agent })
    }

    fn terminal(&self) -> io::Result<Arc<Mutex<Live>>> {
        lock(self.shared)?
            .terminals
            .get(self.id)
            .cloned()
            .ok_or_else(|| io::Error::other("terminal not found"))
    }

    /// Keeps what the CLI said for the cards, beside what the log said then.
    fn record(&self, changed: Changed) -> io::Result<Reply> {
        let Changed {
            model,
            permission,
            permission_full,
            effort,
        } = changed;
        let mut host = lock(self.shared)?;
        let w = host.workspace.clone();
        let session = w.current_agent(self.id);
        let content = session.and_then(|s| w.content.get(&s.id));
        let log_model = content.map_or("", |c| c.model.as_str()).to_owned();
        let log_permission = content.map_or("", |c| c.permission.as_str()).to_owned();
        let running = |id: &String| {
            w.terminals
                .iter()
                .any(|t| t.id == *id && t.state == ProcessState::Running)
        };
        let modes = &mut host.recognition.terminal_modes;
        modes.retain(|id, _| running(id));
        let at = modes.entry(self.id.to_owned()).or_default();
        let session = session.map(|s| s.id.clone());
        if at.session != session {
            *at = Default::default();
            at.session = session;
        }
        if let Some(m) = model.clone() {
            at.model = Some((m, log_model));
        }
        if let Some(p) = permission.clone() {
            at.permission = Some((p, log_permission));
            at.permission_full = permission_full;
        }
        Ok(Reply::Modes(ModeReply::Now {
            model: model.unwrap_or_default(),
            permission: permission.unwrap_or_default(),
            permission_full,
            // Cards say no effort: nothing to keep it for.
            effort: effort.unwrap_or_default(),
        }))
    }
}

/// Why this conversation cannot take a key now, in its state's own terms.
fn idle(w: &Workspace, session: &AgentSession) -> Result<(), &'static str> {
    let now = mooring_core::content::now();
    if let Some(c) = w.content.get(&session.id) {
        if !c.approval.is_empty() || !c.waiting.is_empty() {
            return Err("它在问你问题，答完再换");
        }
    }
    match status::of(w, session, now).phase() {
        status::Phase::Working => Err("这一轮还没结束，等它停下再换"),
        status::Phase::Waiting => Err("它在问你问题，答完再换"),
        status::Phase::Idle => Ok(()),
    }
}

/// The command lines of a terminal's shell and what runs below it.
fn descendants(shell: u32) -> io::Result<Vec<String>> {
    let table = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()?;
    let text = String::from_utf8_lossy(&table.stdout);
    let rows: Vec<(u32, u32, &str)> = text
        .lines()
        .filter_map(|l| {
            let mut parts = l.trim_start().splitn(2, char::is_whitespace);
            let pid = parts.next()?.parse().ok()?;
            let rest = parts.next()?.trim_start();
            let (ppid, command) = rest.split_once(char::is_whitespace)?;
            Some((pid, ppid.parse().ok()?, command.trim_start()))
        })
        .collect();
    let mut seen = vec![shell];
    // The shell's own too: `exec claude` runs in its place.
    let mut out: Vec<String> = (rows.iter())
        .filter(|(pid, ..)| *pid == shell)
        .map(|(.., command)| (*command).to_owned())
        .collect();
    let mut at = 0;
    while at < seen.len() {
        for (pid, ppid, command) in &rows {
            if *ppid == seen[at] && !seen.contains(pid) {
                seen.push(*pid);
                out.push((*command).to_owned());
            }
        }
        at += 1;
    }
    Ok(out)
}
