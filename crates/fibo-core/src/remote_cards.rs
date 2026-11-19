//! Running terminals as cards, the way a phone and another machine's window list them:
//! one owner for both, the host's `list` and a window reading that host's own workspace.
//! What only the host can tell (its Agents' input controls, a folder's git branch) is left
//! for it to fill.
use crate::model::*;
use crate::modes::TerminalModes;
use crate::protocol::RecognitionStatus;
use crate::subagents::WorkflowRun;
use crate::{status, status_group};
use fibo_remote::protocol as rp;

/// The spaces listed, and those put aside, as the cards name them.
pub fn spaces(w: &Workspace) -> (Vec<rp::Space>, Vec<rp::Space>) {
    let space = |s: &Space| rp::Space {
        id: s.id.clone(),
        name: s.name.clone(),
        cwd: s.cwd.clone(),
    };
    let (quiet, spaces): (Vec<_>, Vec<_>) = w.spaces.iter().partition(|s| w.space_quiet(&s.id));
    (
        spaces.into_iter().map(space).collect(),
        quiet.into_iter().map(space).collect(),
    )
}

/// Every running terminal outside a space put aside, as a card.
pub fn cards(w: &Workspace, seen: &RecognitionStatus, now: u64) -> Vec<rp::Card> {
    w.terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        // A space put aside stays off phones and other machines, older ones included.
        .filter(|t| !w.terminal_quiet(&t.id))
        .map(|t| card(w, seen, t, now))
        .collect()
}

/// An Agent in `terminal` that no conversation is tied to: what it is, in the host's words.
fn unbound(w: &Workspace, seen: &RecognitionStatus, terminal: &str) -> Option<crate::agent::Agent> {
    (w.current_agent(terminal).is_none())
        .then(|| seen.terminal_agents.get(terminal).copied())
        .flatten()
        .or(seen.terminal_trust.get(terminal).map(|p| p.agent))
}

/// The Agent a card names for `terminal`: its conversation's, else one found running there.
pub fn agent(
    w: &Workspace,
    seen: &RecognitionStatus,
    terminal: &str,
) -> Option<crate::agent::Agent> {
    (w.current_agent(terminal).map(|s| s.runtime)).or_else(|| unbound(w, seen, terminal))
}

/// What each Agent is doing and what it asks.
fn card(w: &Workspace, seen: &RecognitionStatus, t: &Terminal, now: u64) -> rp::Card {
    let session = w.current_agent(&t.id);
    let cwd = w.terminal_cwd(t);
    let content = session.and_then(|s| w.content.get(&s.id));
    let trust = seen.terminal_trust.get(&t.id);
    let said = |s: &str| Some(s.to_owned()).filter(|s| !s.is_empty());
    // What a change here set, until the log says something newer.
    let set = seen
        .terminal_modes
        .get(&t.id)
        .filter(|m| m.session.as_deref() == session.map(|s| s.id.as_str()));
    let log_model = content.map_or("", |c| c.model.as_str());
    let log_permission = content.map_or("", |c| c.permission.as_str());
    let set_permission = set.and_then(|m| TerminalModes::read(&m.permission, log_permission));
    let permission_full = if set_permission.is_some() {
        set.and_then(|m| m.permission_full)
    } else {
        content.and_then(|c| c.permission_full)
    };
    let report = seen.terminal_reports.get(&t.id);
    let status = status::of_terminal(w, &t.id, trust.is_some(), report, now);
    let phase = match status.map(status::Status::phase) {
        Some(status::Phase::Working) => rp::Phase::Working,
        Some(status::Phase::Waiting) => rp::Phase::Waiting,
        Some(status::Phase::Idle) => rp::Phase::Idle,
        None => rp::Phase::Shell,
    };
    let unbound = unbound(w, seen, &t.id);
    let note = unbound
        .filter(|_| trust.is_none())
        .and_then(|_| seen.terminal_notes.get(&t.id).map(|n| n.note.clone()));
    let agent = agent(w, seen, &t.id);
    rp::Card {
        agent_name: agent.map_or_else(String::new, |a| a.name().to_owned()),
        controls: None,
        id: t.id.clone(),
        // The space the window lists the conversation under; a shell's own.
        space: session
            .and_then(|s| w.session_space(s))
            .unwrap_or(&t.space)
            .to_owned(),
        tab: w.terminal_tab_title(&t.id),
        cwd: cwd.clone(),
        phase,
        agent: agent.map(|a| a.key().to_lowercase()),
        session: session.map(|s| s.id.clone()),
        title: Some(
            session
                .map(|s| w.session_title(s))
                .or_else(|| note.clone())
                .unwrap_or_else(|| cwd.clone()),
        ),
        approval: content.and_then(|c| said(&c.approval)),
        waiting: content.and_then(|c| asked(c).or_else(|| said(crate::i18n::say(&c.waiting)))),
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
        branch: None,
        running_since: session.and_then(|s| status::running_since(w, s, now)),
        // Its first ask is the card's title already; only a record says more.
        doing: content
            .filter(|_| status == Some(status::Status::Running))
            .and_then(|c| c.doing_recorded())
            // A finished turn's row says what it left running, as the sidebar does.
            .or_else(|| {
                session
                    .filter(|_| status == Some(status::Status::Review))
                    .and_then(|s| status::left_behind(w, s))
            }),
        workflows: content
            .map(|c| {
                c.subagents
                    .iter()
                    .filter_map(|s| s.workflow_summary(status == Some(status::Status::Running)))
                    .collect()
            })
            .unwrap_or_default(),
        model: (set.and_then(|m| TerminalModes::read(&m.model, log_model)))
            .or_else(|| content.and_then(|c| said(&c.model))),
        permission_full,
        permission: set_permission
            .or_else(|| content.and_then(|c| said(&c.permission)))
            .map(|p| crate::i18n::say(&p).to_owned()),
        subagents: content.map(|c| c.subagents.clone()).unwrap_or_default(),
    }
}

/// A question tool's question with its options as numbered lines (`1. 新对话继续`), the
/// way a phone offers a screen's options as buttons and answers one with its number key.
/// The card keeps them apart; as one text, a phone from before this reads them too.
fn asked(c: &crate::content::Content) -> Option<String> {
    if c.choices.is_empty() || c.waiting.is_empty() {
        return None;
    }
    let first = c.waiting.lines().next().unwrap_or_default();
    let listed = format!(
        "{}{}{}",
        not_ui!("（"),
        c.choices.join(" / "),
        not_ui!("）")
    );
    let question = first.strip_suffix(listed.as_str()).unwrap_or(first);
    let options = (c.choices.iter().take(9).enumerate())
        .map(|(i, o)| format!("{}. {}", i + 1, o.replace('\n', " ")))
        .collect::<Vec<_>>();
    Some(format!("{question}\n{}", options.join("\n")))
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
        let c = crate::content::Content {
            waiting: "【下一步】接下来怎么继续？（新对话继续 / 就在这里继续）".into(),
            choices: vec!["新对话继续".into(), "就在这里继续".into()],
            ..Default::default()
        };
        assert_eq!(
            asked(&c).as_deref(),
            Some("【下一步】接下来怎么继续？\n1. 新对话继续\n2. 就在这里继续")
        );
        let approval = crate::content::Content {
            waiting: "CLI 请求工具审批（Hook）".into(),
            ..Default::default()
        };
        assert_eq!(asked(&approval), None, "no options: the text as it is");
    }

    /// A shell whose program reports its own state reaches the phone with that state, as
    /// the window's row shows it, not as a bare 终端.
    #[test]
    fn a_reporting_program_reaches_the_phone_with_its_state() {
        let mut w = crate::sample::every_state();
        let shell = "pty-deploy".to_owned();
        w.terminals.push(Terminal {
            id: shell.clone(),
            space: "space".into(),
            argv: vec!["/bin/zsh".into()],
            cwd: "/project".into(),
            pid: None,
            state: ProcessState::Running,
            exit_code: None,
            error: None,
        });
        let mut seen = RecognitionStatus::default();
        let card = |seen: &RecognitionStatus| {
            let t = w.terminals.iter().find(|t| t.id == shell).unwrap();
            card(&w, seen, t, crate::content::now())
        };
        assert_eq!(card(&seen).phase, rp::Phase::Shell);
        seen.terminal_reports.insert(
            shell.clone(),
            crate::protocol::ProgramReport {
                state: "blocked".into(),
                kind: None,
                progress: None,
                app: String::new(),
                title: String::new(),
                msg: String::new(),
                since: 0,
            },
        );
        let blocked = card(&seen);
        assert_eq!(blocked.phase, rp::Phase::Waiting);
        assert_eq!(blocked.status.as_deref(), Some("waiting"));
    }
}
