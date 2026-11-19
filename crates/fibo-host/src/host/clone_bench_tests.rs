//! What the host's whole-workspace copies cost on a long-used install: 2000 sessions, half
//! older than the recent window, each with a card of real size (about 5 KB as JSON, as the
//! window's snapshot of 842 sessions was 4.34 MB). Retired instructions of this process, old
//! and new way alternating; run alone, by hand:
//! `cargo test --release -p fibo-host --lib clone_bench -- --ignored --nocapture --test-threads=1`.
use super::tests::session;
use super::*;
use fibo_core::content::{Content, Evidence, Quote, Source, Touch, Turn};

pub(super) fn big_workspace(n: usize) -> Workspace {
    let now = fibo_core::content::now();
    let mut w = Workspace::default();
    let source = |i: usize| Source {
        path: format!("/Users/someone/.codex/sessions/2026/11/19/rollout-{i:08}.jsonl"),
        inode: i as u64,
        offset: 4096,
    };
    for i in 0..n {
        let id = format!("019a{i:04}-5c1e-7d40-9b8e-2f6a3c1d{i:04}");
        let live = i % 33 == 0;
        let updated = if i % 2 == 0 {
            now - 60
        } else {
            now - 30 * 86400
        };
        let mut s = session(&id, updated, None, live);
        s.terminal = live.then(|| format!("terminal-{i}"));
        s.cwd = format!("/Users/someone/code/project-{}", i % 40);
        s.transcript = source(i).path;
        w.agent_sessions.push(s);
        let words = |k: usize| "change the sidebar row so it reads well ".repeat(k);
        w.content.insert(
            id,
            Content {
                title: words(2),
                asks: (0..3)
                    .map(|k| Quote {
                        text: words(8),
                        at: updated - k,
                        source: source(i),
                    })
                    .collect(),
                evidence: (0..8)
                    .map(|k| Evidence {
                        kind: "test".into(),
                        summary: words(4),
                        exit_code: Some(0),
                        at: updated - k,
                        source: source(i),
                    })
                    .collect(),
                last_reply: words(36),
                turn: Turn::Completed,
                turn_started: updated - 300,
                turn_key: format!("{i}:turn"),
                updated,
                files: (0..20)
                    .map(|k| Touch {
                        path: format!("crates/fibo-app/src/workbench/file_{k}.rs"),
                        at: updated,
                        added: 3,
                        removed: 1,
                    })
                    .collect(),
                model: "gpt-5-codex".into(),
                ..Content::default()
            },
        );
    }
    w
}

pub(super) fn instructions() -> u64 {
    fibo_platform::inspect::usage(std::process::id()).map_or(0, |u| u.instructions)
}

/// Instructions per call of each of `ways`, each run `rounds` times in turn, `n` calls a run.
pub(super) fn alternate(rounds: usize, n: usize, ways: &mut [(&str, &mut dyn FnMut())]) {
    for round in 0..rounds {
        for (name, f) in ways.iter_mut() {
            let at = instructions();
            for _ in 0..n {
                f();
            }
            let per = (instructions() - at) / n as u64;
            eprintln!("round {round} {name}: {:.2} M per call", per as f64 / 1e6);
        }
    }
}

/// A log batch's question "did content change": the whole table against the cards it
/// read (one, the usual batch), with the copy `apply` makes beside them for scale.
#[test]
#[ignore = "a measurement, by hand"]
fn clone_bench_apply() {
    let w = big_workspace(2000);
    let next = w.clone();
    let touched = [w.agent_sessions[7].id.clone()];
    alternate(
        3,
        20,
        &mut [
            ("copy", &mut || {
                std::hint::black_box(w.clone());
            }),
            ("whole table", &mut || {
                std::hint::black_box(next.content != w.content);
            }),
            ("cards read", &mut || {
                std::hint::black_box(
                    (touched.iter()).any(|id| next.content.get(id) != w.content.get(id)),
                );
            }),
        ],
    );
}
