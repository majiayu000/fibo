//! `list`: spaces, running terminals as cards, and the board's todos. Called often (every
//! burst of events), so everything here reads what the host already has, and a card's
//! branch comes from `.git/HEAD` through a short cache, never a git process.
use crate::control::{self, Phase};
use mooring_core::{git_head, model::*, protocol::*, status, status_group};
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a directory's branch is believed: a checkout shows within this.
const BRANCH_FOR: Duration = Duration::from_secs(3);
/// Directories kept; past this the cache starts over.
const BRANCHES: usize = 256;
type Branches = HashMap<String, (Instant, Option<String>)>;
static BRANCH: Mutex<Option<Branches>> = Mutex::new(None);

fn branch(cwd: &str) -> Option<String> {
    let mut cache = BRANCH.lock().ok()?;
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((at, branch)) = cache.get(cwd) {
        if at.elapsed() < BRANCH_FOR {
            return branch.clone();
        }
    }
    if cache.len() >= BRANCHES {
        cache.clear();
    }
    let read = git_head::read_branch(Path::new(cwd));
    cache.insert(cwd.to_owned(), (Instant::now(), read.clone()));
    read
}

/// Spaces and running terminals as cards: what each Agent is doing and what it asks.
pub(super) fn list(w: &Workspace, seen: &RecognitionStatus, socket: &Path) -> io::Result<Value> {
    let now = mooring_core::content::now();
    let terminals: Vec<_> = w
        .terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        .map(|t| {
            let session = w.current_agent(&t.id);
            let cwd = w.terminal_cwd(t);
            let content = session.and_then(|s| w.content.get(&s.id));
            let phase = match control::phase(w, &t.id) {
                Phase::Working => rp::Phase::Working,
                Phase::Waiting => rp::Phase::Waiting,
                Phase::Idle => rp::Phase::Idle,
                Phase::None => rp::Phase::Shell,
            };
            let said = |s: &String| Some(s.clone()).filter(|s| !s.is_empty());
            let status = control::status(w, &t.id);
            // An Agent here that no conversation is tied to: what it is, in the host's words.
            let unbound = session
                .is_none()
                .then(|| seen.terminal_agents.get(&t.id).copied())
                .flatten();
            let note = unbound.and_then(|_| seen.terminal_notes.get(&t.id).map(|n| n.note.clone()));
            rp::Card {
                id: t.id.clone(),
                // The space the window lists the conversation under; a shell's own.
                space: session
                    .and_then(|s| w.session_space(s))
                    .unwrap_or(&t.space)
                    .to_owned(),
                tab: control::tab_title(w, &t.id),
                cwd: cwd.clone(),
                phase,
                agent: session
                    .map(|s| s.runtime)
                    .or(unbound)
                    .map(|a| control::agent_label(a).to_owned()),
                session: session.map(|s| s.id.clone()),
                title: session.map(|s| w.session_title(s)),
                approval: content.and_then(|c| said(&c.approval)),
                waiting: content.and_then(|c| said(&c.waiting)),
                status: status.map(|s| s.key().to_owned()),
                status_label: status.map(|s| s.label().to_owned()),
                needs_you: session.is_some_and(|s| w.needs_you(s, now)),
                column: session
                    .and_then(|s| status_group::column(w, s, now))
                    .map(|c| c.key().to_owned()),
                note,
                turn: content
                    .map(|c| c.turn_key.clone())
                    .filter(|t| !t.is_empty()),
                outcome: session.map(|s| outcome_key(&s.outcome).to_owned()),
                updated: session.map(|s| w.session_updated(s)),
                branch: branch(&cwd),
                running_since: session.and_then(|s| status::running_since(w, s, now)),
                // Its first ask is the card's title already; only a record says more.
                doing: content
                    .filter(|_| status == Some(status::Status::Running))
                    .and_then(|c| c.doing())
                    .filter(|d| !d.starts_with("做 ")),
                model: content.and_then(|c| said(&c.model)),
                permission: content.and_then(|c| said(&c.permission)),
            }
        })
        .collect();
    let spaces = w
        .spaces
        .iter()
        .map(|s| rp::Space {
            id: s.id.clone(),
            name: s.name.clone(),
            cwd: s.cwd.clone(),
        })
        .collect();
    let todos = (status::open_todos(w, now).into_iter())
        .map(|t| rp::Todo {
            id: t.id.clone(),
            space: t.space.clone(),
            title: t.title.clone(),
            agent: control::agent_label(t.agent),
        })
        .collect();
    Ok(json!(rp::List {
        machine: super::machine(socket)?,
        spaces,
        terminals,
        todos,
    }))
}

fn outcome_key(o: &Outcome) -> &'static str {
    match o {
        Outcome::Open => "open",
        Outcome::Completed => "done",
        Outcome::Deferred { .. } => "deferred",
        Outcome::Abandoned => "abandoned",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The phone's cards say what the window says, on every sample case.
    #[test]
    fn cards_read_every_field_from_its_owner() {
        let w = mooring_core::sample::every_state();
        let dir = std::env::temp_dir().join(format!("mooring-cards-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list: rp::List =
            serde_json::from_value(list(&w, &Default::default(), &dir.join("sock")).unwrap())
                .unwrap();
        let at = mooring_core::content::now();
        assert!(!list.terminals.is_empty());
        for card in &list.terminals {
            let term = w.terminals.iter().find(|t| t.id == card.id).unwrap();
            assert_eq!(card.tab, w.terminal_tab_title(&card.id), "{}", card.id);
            assert_eq!(card.cwd, w.terminal_cwd(term), "{}", card.id);
            let Some(s) = w.current_agent(&card.id) else {
                assert_eq!(card.phase, rp::Phase::Shell);
                continue;
            };
            let st = status::of(&w, s, at);
            assert_eq!(card.title.as_deref(), Some(w.session_title(s).as_str()));
            assert_eq!(card.status.as_deref(), Some(st.key()), "{}", card.id);
            assert_eq!(
                card.status_label.as_deref(),
                Some(st.label()),
                "{}",
                card.id
            );
            assert_eq!(card.needs_you, w.needs_you(s, at), "{}", card.id);
            assert_eq!(card.space, w.session_space(s).unwrap_or(&term.space));
            assert_eq!(
                card.column.as_deref(),
                status_group::column(&w, s, at).map(|c| c.key()),
                "{}",
                card.id
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A Codex on the shared daemon has no conversation, yet it is no plain shell.
    #[test]
    fn an_agent_without_a_conversation_lists_with_its_note() {
        let mut w = mooring_core::sample::every_state();
        let shell = w
            .terminals
            .iter()
            .find(|t| t.state == ProcessState::Running)
            .unwrap()
            .id
            .clone();
        w.agent_sessions
            .retain(|s| s.terminal.as_deref() != Some(shell.as_str()));
        assert!(w.current_agent(&shell).is_none());
        let mut seen = RecognitionStatus::default();
        seen.terminal_agents
            .insert(shell.clone(), mooring_core::agent::Agent::Codex);
        seen.terminal_notes.insert(
            shell.clone(),
            TerminalNote {
                note: "codex · 空闲 · 看不到对话".into(),
                why: String::new(),
            },
        );
        let dir = std::env::temp_dir().join(format!("mooring-unbound-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list: rp::List =
            serde_json::from_value(list(&w, &seen, &dir.join("sock")).unwrap()).unwrap();
        let card = list.terminals.iter().find(|c| c.id == shell).unwrap();
        assert_eq!(card.phase, rp::Phase::Shell);
        assert_eq!(card.agent.as_deref(), Some("codex"));
        assert_eq!(card.note.as_deref(), Some("codex · 空闲 · 看不到对话"));
        assert!(list
            .terminals
            .iter()
            .filter(|c| c.id != shell)
            .all(|c| c.note.is_none()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
