//! `list`: spaces, running terminals as cards, and the board's todos. Called often (every
//! burst of events), so everything here reads what the host already has, and a card's
//! branch comes from `.git/HEAD` through a short cache, never a git process.
use crate::control;
use mooring_core::modes::TerminalModes;
use mooring_core::{git_head, model::*, protocol::*, status, status_group};
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A question tool's question with its options as numbered lines (`1. 新对话继续`), the
/// way a phone offers a screen's options as buttons and answers one with its number key.
/// The card keeps them apart; as one text, a phone from before this reads them too.
fn asked(c: &mooring_core::content::Content) -> Option<String> {
    if c.choices.is_empty() || c.waiting.is_empty() {
        return None;
    }
    let first = c.waiting.lines().next().unwrap_or_default();
    let listed = format!("（{}）", c.choices.join(" / "));
    let question = first.strip_suffix(listed.as_str()).unwrap_or(first);
    let options = (c.choices.iter().take(9).enumerate())
        .map(|(i, o)| format!("{}. {}", i + 1, o.replace('\n', " ")))
        .collect::<Vec<_>>();
    Some(format!("{question}\n{}", options.join("\n")))
}

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
        // A space put aside stays off phones and other machines, older ones included.
        .filter(|t| !w.terminal_quiet(&t.id))
        .map(|t| {
            let session = w.current_agent(&t.id);
            let cwd = w.terminal_cwd(t);
            let content = session.and_then(|s| w.content.get(&s.id));
            let trust = seen.terminal_trust.get(&t.id);
            let said = |s: &String| Some(s.clone()).filter(|s| !s.is_empty());
            // What a change here set, until the log says something newer.
            let set = seen
                .terminal_modes
                .get(&t.id)
                .filter(|m| m.session.as_deref() == session.map(|s| s.id.as_str()));
            let log_model = content.map_or("", |c| c.model.as_str());
            let log_permission = content.map_or("", |c| c.permission.as_str());
            let status = status::of_terminal(w, &t.id, trust.is_some(), now);
            let phase = match status.map(status::Status::phase) {
                Some(status::Phase::Working) => rp::Phase::Working,
                Some(status::Phase::Waiting) => rp::Phase::Waiting,
                Some(status::Phase::Idle) => rp::Phase::Idle,
                None => rp::Phase::Shell,
            };
            // An Agent here that no conversation is tied to: what it is, in the host's words.
            let unbound = session
                .is_none()
                .then(|| seen.terminal_agents.get(&t.id).copied())
                .flatten()
                .or(trust.map(|p| p.agent));
            let note = unbound
                .filter(|_| trust.is_none())
                .and_then(|_| seen.terminal_notes.get(&t.id).map(|n| n.note.clone()));
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
                waiting: content.and_then(|c| asked(c).or_else(|| said(&c.waiting))),
                status: status.map(|s| s.key().to_owned()),
                status_label: status.map(|s| s.label().to_owned()),
                needs_you: trust.is_some() || session.is_some_and(|s| w.needs_you(s, now)),
                trust: trust.map(|p| rp::TrustAsk {
                    question: p.question.clone(),
                    folder: p.folder.clone(),
                }),
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
                    .filter(|d| !d.starts_with("做 "))
                    // A finished turn's row says what it left running, as the sidebar does.
                    .or_else(|| {
                        session
                            .filter(|_| status == Some(status::Status::Review))
                            .and_then(|s| status::left_behind(w, s))
                    }),
                model: (set.and_then(|m| TerminalModes::read(&m.model, log_model)))
                    .or_else(|| content.and_then(|c| said(&c.model))),
                permission: (set.and_then(|m| TerminalModes::read(&m.permission, log_permission)))
                    .or_else(|| content.and_then(|c| said(&c.permission))),
            }
        })
        .collect();
    let spaces = w
        .spaces
        .iter()
        .filter(|s| !w.space_quiet(&s.id))
        .map(|s| rp::Space {
            id: s.id.clone(),
            name: s.name.clone(),
            cwd: s.cwd.clone(),
        })
        .collect();
    let todos = (status::unfinished_todos(w, now).into_iter())
        .map(|t| todo(w, t, now))
        .collect();
    Ok(json!(rp::List {
        machine: super::machine(socket)?,
        spaces,
        terminals,
        todos,
    }))
}

/// A todo with what the phone shows of it: its state, your words, the progress so far, and
/// the terminal of a conversation working on it.
fn todo(w: &Workspace, t: &Task, now: u64) -> rp::Todo {
    let state = status::task_state(w, t, now);
    let log = w.task_logs.get(&t.id);
    let terminal = (w.agent_sessions.iter())
        .filter(|s| s.task.as_ref() == Some(&t.id))
        .filter_map(|s| status::live_terminal(w, s))
        .map(|term| term.id.clone())
        .next();
    rp::Todo {
        id: t.id.clone(),
        space: t.space.clone(),
        title: t.title.clone(),
        agent: control::agent_label(t.agent),
        state: Some(state.key().to_owned()),
        state_label: Some(state.label().to_owned()),
        said: t.description.trim().to_owned(),
        acceptance: t.acceptance.iter().map(|c| c.description.clone()).collect(),
        notes: (log.map(|l| l.notes.as_slice()).unwrap_or_default().iter())
            .map(|n| rp::TodoNote {
                at: n.at,
                text: n.text.clone(),
            })
            .collect(),
        created: log.and_then(|l| l.created),
        terminal,
    }
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

    /// The phone turns numbered lines into buttons (`choices` in mobile/src/ui/parse.ts).
    #[test]
    fn a_question_reaches_the_phone_with_numbered_options() {
        let c = mooring_core::content::Content {
            waiting: "【下一步】接下来怎么继续？（新对话继续 / 就在这里继续）".into(),
            choices: vec!["新对话继续".into(), "就在这里继续".into()],
            ..Default::default()
        };
        assert_eq!(
            asked(&c).as_deref(),
            Some("【下一步】接下来怎么继续？\n1. 新对话继续\n2. 就在这里继续")
        );
        let approval = mooring_core::content::Content {
            waiting: "CLI 请求工具审批（Hook）".into(),
            ..Default::default()
        };
        assert_eq!(asked(&approval), None, "no options: the text as it is");
    }

    /// A space put aside stays off phones and other machines, its todos too.
    #[test]
    fn a_space_put_aside_is_not_listed_to_other_devices() {
        let mut w = mooring_core::sample::every_state();
        w.spaces.push(Space {
            id: "other".into(),
            name: "另一个".into(),
            cwd: "/other".into(),
        });
        let dir = std::env::temp_dir().join(format!("mooring-quiet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let read = |w: &Workspace| -> rp::List {
            serde_json::from_value(list(w, &Default::default(), &dir.join("sock")).unwrap())
                .unwrap()
        };
        let all = read(&w);
        assert!(!all.terminals.is_empty());
        w.edit(Edit::SetSpaceQuiet {
            id: "space".into(),
            quiet: true,
        })
        .unwrap();
        let quiet = read(&w);
        assert!(quiet.terminals.is_empty());
        assert!(quiet.todos.is_empty());
        assert_eq!(quiet.spaces.len(), 1);
        assert_eq!(quiet.spaces[0].id, "other");
        w.edit(Edit::SetSpaceQuiet {
            id: "space".into(),
            quiet: false,
        })
        .unwrap();
        assert_eq!(read(&w).terminals.len(), all.terminals.len());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 接着做 goes to the conversation working on a todo while its terminal is open.
    #[test]
    fn a_todo_names_the_open_terminal_working_on_it() {
        let mut w = mooring_core::sample::every_state();
        let session = (w.agent_sessions.iter())
            .position(|s| status::live_terminal(&w, s).is_some())
            .expect("a sample session in an open terminal");
        let space = w.spaces[0].id.clone();
        w.tasks.push(Task {
            id: "phone-todo".into(),
            space,
            title: "修键盘".into(),
            description: String::new(),
            acceptance: vec![],
            agent: Agent::Claude,
        });
        let now = mooring_core::content::now();
        assert_eq!(todo(&w, w.tasks.last().unwrap(), now).terminal, None);
        w.agent_sessions[session].task = Some("phone-todo".into());
        let live = status::live_terminal(&w, &w.agent_sessions[session]).map(|t| t.id.clone());
        let t = todo(&w, w.tasks.last().unwrap(), now);
        assert_eq!(t.terminal, live);
        assert_eq!(
            t.state_label.as_deref(),
            Some(status::task_state(&w, w.tasks.last().unwrap(), now).label())
        );
    }

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
    /// The phone's row says what a finished turn left running, in the sidebar's words.
    #[test]
    fn a_card_says_what_its_finished_turn_left_running() {
        let mut w = mooring_core::sample::every_state();
        let session = (w.agent_sessions.iter())
            .find(|s| status::live_terminal(&w, s).is_some() && s.id == "review")
            .expect("the sample has a live finished turn")
            .id
            .clone();
        let c = w.content.get_mut(&session).unwrap();
        c.background = Some(mooring_core::content::Background {
            shells: 1,
            ..Default::default()
        });
        c.shells = vec![mooring_core::content::ShellTask {
            id: "server".into(),
            port: Some(5569),
            ..Default::default()
        }];
        let dir = std::env::temp_dir().join(format!("mooring-left-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list: rp::List =
            serde_json::from_value(list(&w, &Default::default(), &dir.join("sock")).unwrap())
                .unwrap();
        let card = (list.terminals.iter())
            .find(|c| c.session.as_deref() == Some(session.as_str()))
            .unwrap();
        assert_eq!(card.status.as_deref(), Some("review"));
        assert_eq!(card.doing.as_deref(), Some("后台留着 1 个服务 · 端口 5569"));
    }

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
                note: "codex · 空闲 · 未关联对话".into(),
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
        assert_eq!(card.note.as_deref(), Some("codex · 空闲 · 未关联对话"));
        assert!(list
            .terminals
            .iter()
            .filter(|c| c.id != shell)
            .all(|c| c.note.is_none()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
