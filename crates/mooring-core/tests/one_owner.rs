//! Each fact about a conversation has one owner in core; these check the owners agree
//! with each other on every sample case and after any sequence of edits. The surfaces
//! (window, menu bar, CLI, phone, plugins) have their own tests that they read these.
use mooring_core::content::now;
use mooring_core::model::*;
use mooring_core::sample::{CASES, every_state};
use mooring_core::status::{self, Status};

/// A tab holding one live conversation is called what that conversation is called.
fn names_agree(w: &Workspace) -> Result<(), String> {
    let none = std::collections::HashMap::new();
    for s in w.agent_sessions.iter().filter(|s| s.current) {
        let Some(tab) = w.sole_session_tab(s) else {
            continue;
        };
        let (tab_name, name) = (w.tab_title(tab, &none, None), w.session_title(s));
        if w.session_name(s).is_some() && tab_name != name {
            return Err(format!(
                "{}: tab says {tab_name:?}, sidebar says {name:?}",
                s.id
            ));
        }
        if !tab.title.is_empty() {
            return Err(format!("{}: tab {} still keeps its own name", s.id, tab.id));
        }
    }
    Ok(())
}

#[test]
fn every_sample_reads_one_name_one_state() {
    let w = every_state();
    names_agree(&w).unwrap();
    let at = now();
    let by = |id: &str| w.agent_sessions.iter().find(|s| s.id == id).unwrap();
    assert_eq!(w.session_title(by("renamed")), "我起的名");
    assert_eq!(w.session_title(by("tasked")), "修登录");
    // A name pinned on the tab before names had one owner moved onto its conversation.
    assert_eq!(w.session_title(by("tab-named")), "部署");
    assert_eq!(w.terminal_tab_title("pty-tab-named"), "部署");
    // The phone and the CLI read the same folder the sidebar does.
    let sub = w.terminals.iter().find(|t| t.id == "pty-subdir").unwrap();
    assert_eq!(w.terminal_cwd(sub), "/project/web");
    // 要你处理 is waiting or failed, live in a tab, not put off, closed or hidden.
    let needs: Vec<_> = CASES
        .iter()
        .filter(|c| w.needs_you(by(c), at))
        .copied()
        .collect();
    assert_eq!(needs, ["waiting", "error"]);
    for case in CASES {
        let s = by(case);
        let st = status::of(&w, s, at);
        assert_eq!(st.phase(), st.phase(), "phase is a function of the status");
        if w.needs_you(s, at) {
            assert!(
                st.needs_you(),
                "{case}: counted as needing you while {st:?}"
            );
        }
    }
    assert_eq!(status::of(&w, by("background"), at), Status::Background);
    assert_eq!(status::of(&w, by("closed"), at), Status::Closed);
}

/// A tiny deterministic generator, so a failure names the seed that reproduces it.
struct Lcg(u64);
impl Lcg {
    fn pick(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

/// Renaming from the tab, renaming from the sidebar, a new conversation in the same
/// terminal, marking done or putting off: after every step the tab and the sidebar
/// still name each conversation the same, and nothing counts as needing you that is
/// not waiting or failed.
#[test]
fn any_sequence_of_edits_keeps_one_name() {
    for seed in 0..200u64 {
        let mut w = every_state();
        let mut rng = Lcg(seed);
        for step in 0..40 {
            let live: Vec<AgentSession> = w
                .agent_sessions
                .iter()
                .filter(|s| s.current)
                .cloned()
                .collect();
            let s = &live[rng.pick(live.len())];
            let name = ["", "甲", "乙 丙", "部署"][rng.pick(4)].to_owned();
            match rng.pick(5) {
                // The tab's rename field: a tab holding one conversation renames it.
                0 => match w.terminal_tab(s.terminal.as_deref().unwrap()).cloned() {
                    Some(_) if w.sole_session_tab(s).is_some() => w
                        .edit(Edit::RenameSession {
                            id: s.id.clone(),
                            name,
                        })
                        .unwrap(),
                    Some(mut tab) => {
                        tab.title = name;
                        w.edit(Edit::PutTab { tab }).unwrap()
                    }
                    None => (),
                },
                1 => w
                    .edit(Edit::RenameSession {
                        id: s.id.clone(),
                        name,
                    })
                    .unwrap(),
                // A new conversation starts in the same terminal.
                2 => {
                    let mut next = s.clone();
                    next.id = format!("{}-{step}", s.id);
                    next.cli_session_id = format!("cli-{}-{step}", s.id);
                    next.name = String::new();
                    next.outcome = Outcome::Open;
                    w.observe_agent(next).unwrap();
                }
                3 => w
                    .edit(Edit::SetOutcome {
                        id: s.id.clone(),
                        outcome: Outcome::Completed,
                    })
                    .unwrap_or(()),
                _ => (),
            }
            names_agree(&w).unwrap_or_else(|e| panic!("seed {seed} step {step}: {e}"));
            let at = now();
            for s in &w.agent_sessions {
                if w.needs_you(s, at) {
                    assert!(status::of(&w, s, at).needs_you(), "seed {seed} step {step}");
                }
            }
        }
    }
}
