use super::*;
use std::time::Instant;

fn log(cli_id: &str, updated: u64) -> Transcript {
    Transcript {
        runtime: Agent::Claude,
        cli_id: cli_id.into(),
        path: format!("/logs/{cli_id}.jsonl"),
        cwd: "/w".into(),
        updated,
    }
}

/// Undoing the skip in `reconcile` observes every known log on each refresh again: 42 ms
/// under the host lock on a long-used install (4478 sessions, 9648 logs).
#[test]
fn a_log_that_did_not_move_is_not_observed_again() {
    let mut w = Workspace::default();
    let logs: Vec<_> = (0..3).map(|i| log(&format!("c{i}"), 10)).collect();
    for t in &logs {
        w.observe_agent(observation(t, None)).unwrap();
    }
    let revision = w.revision;
    reconcile(&mut w, &logs, &[], &HashMap::new()).unwrap();
    assert_eq!(w.revision, revision);
    let moved = [log("c1", 20), log("c2", 5)];
    reconcile(&mut w, &moved, &[], &HashMap::new()).unwrap();
    assert_eq!(w.revision, revision + 1);
    let updated = |id: &str| {
        w.agent_sessions
            .iter()
            .find(|s| s.cli_session_id == id)
            .unwrap()
            .updated
    };
    assert_eq!((updated("c1"), updated("c2")), (20, 10));
}

/// The loop before the skip, kept to check the new one against real data.
fn observe_all(w: &mut Workspace, transcripts: &[Transcript]) {
    let known: std::collections::HashSet<_> = w
        .agent_sessions
        .iter()
        .map(|s| (s.runtime, s.cli_session_id.clone()))
        .collect();
    for t in transcripts {
        if known.contains(&(t.runtime, t.cli_id.clone())) {
            w.observe_agent(observation(t, None)).unwrap();
        }
    }
}

/// What one `refresh_agents` costs while holding the host lock, on a copy of a real
/// install's store, and that skipping unmoved logs leaves the same sessions. By hand:
/// `MOORING_REFRESH_BENCH=<dir holding a copy of workbench.sqlite> cargo test --release -p fibo-host --lib refresh_bench -- --ignored --nocapture`.
#[test]
#[ignore]
fn refresh_bench() {
    let root = std::path::PathBuf::from(std::env::var("MOORING_REFRESH_BENCH").unwrap());
    let (_store, mut w) = crate::store::Store::open(&root).unwrap();
    let index = crate::log_index::Index::open(&root, vec![]).unwrap();
    fibo_core::retention::archive(&mut w, fibo_core::content::now(), &Default::default());
    let transcripts = index.cached().unwrap();
    println!(
        "sessions {} transcripts {}",
        w.agent_sessions.len(),
        transcripts.len()
    );
    // The host keeps what the last refresh made; measure the refresh after that one.
    let mut settled = w.clone();
    reconcile(&mut settled, &transcripts, &[], &HashMap::new()).unwrap();
    for _ in 0..5 {
        let mut old = settled.clone();
        let t = Instant::now();
        observe_all(&mut old, &transcripts);
        let before = t.elapsed();
        let mut new = settled.clone();
        let t = Instant::now();
        reconcile(&mut new, &transcripts, &[], &HashMap::new()).unwrap();
        let after = t.elapsed();
        println!("all logs {before:?}, moved logs only {after:?}");
        assert!(old.agent_sessions == new.agent_sessions && old.tabs == new.tabs);
    }
    // From the store as loaded, where logs have moved since it was saved.
    let (mut old, mut new) = (w.clone(), w.clone());
    observe_all(&mut old, &transcripts);
    let t = Instant::now();
    reconcile(&mut new, &transcripts, &[], &HashMap::new()).unwrap();
    println!("first refresh after start {:?}", t.elapsed());
    assert!(old.agent_sessions == new.agent_sessions && old.tabs == new.tabs);
}
