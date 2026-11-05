use super::*;
use crate::model::{Agent, Outcome};
use crate::status::{Status, of};
fn status(c: &Content, s: &AgentSession, now: u64) -> Status {
    let mut w = Workspace::default();
    w.content.insert(s.id.clone(), c.clone());
    of(&w, s, now)
}
#[test]
fn grouping_priority_and_deferred_expiry() {
    let now = 10 * 86400;
    let mut s = AgentSession {
        id: "s".into(),
        runtime: Agent::Codex,
        cli_session_id: "c".into(),
        terminal: None,
        current: false,
        task: None,
        outcome: Outcome::Open,
        updated: now,
        cwd: "/tmp".into(),
        transcript: "f".into(),
        binding: None,
        name: String::new(),
    };
    let mut c = Content {
        updated: now,
        turn: Turn::Waiting,
        ..Default::default()
    };
    assert_eq!(status(&c, &s, now), Status::Waiting);
    c.turn = Turn::Error;
    assert_eq!(status(&c, &s, now), Status::Error);
    c.turn = Turn::Completed;
    c.completed_claim = true;
    assert_eq!(status(&c, &s, now), Status::Review);
    // A finished turn waits for you either way: done is yours to say.
    c.completed_claim = false;
    assert_eq!(status(&c, &s, now), Status::Review);
    c.turn = Turn::Unknown;
    assert_eq!(status(&c, &s, now), Status::Paused);
    c.turn = Turn::Running;
    assert_eq!(status(&c, &s, now), Status::Running);
    c.updated = 1;
    assert_eq!(status(&c, &s, now), Status::Stale);
    s.outcome = Outcome::Deferred {
        until: now + 1,
        reason: "later".into(),
    };
    assert_eq!(status(&c, &s, now), Status::Deferred);
    s.outcome = Outcome::Deferred {
        until: now - 1,
        reason: "later".into(),
    };
    assert_eq!(status(&c, &s, now), Status::Paused);
    s.outcome = Outcome::Completed;
    assert_eq!(status(&c, &s, now), Status::Closed);
}
#[test]
fn running_subagents_keep_a_stopped_turn_working_and_the_rest_wait_on_background() {
    let now = 10 * 86400;
    let s = AgentSession {
        id: "s".into(),
        runtime: Agent::Claude,
        cli_session_id: "c".into(),
        terminal: Some("t".into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: now,
        cwd: "/tmp".into(),
        transcript: "f".into(),
        binding: None,
        name: String::new(),
    };
    let mut c = Content {
        updated: now,
        turn: Turn::Completed,
        completed_claim: true,
        background: Some(Background {
            agents: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(status(&c, &s, now), Status::Running);
    c.background = Some(Background {
        monitors: 1,
        ..Default::default()
    });
    assert_eq!(
        status(&c, &s, now),
        Status::Background,
        "a monitor will wake it"
    );
    assert!(c.waking());
    assert!(
        c.here()
            .stopped
            .starts_with("等待唤醒，后台仍有监视任务 1 个")
    );
    c.background = Some(Background {
        crons: 1,
        ..Default::default()
    });
    assert_eq!(
        status(&c, &s, now),
        Status::Background,
        "so will a scheduled wakeup"
    );
    c.background = Some(Background {
        shells: 2,
        ..Default::default()
    });
    assert!(!c.waking());
    assert_eq!(
        status(&c, &s, now),
        Status::Background,
        "a shell it started still runs"
    );
    c.background = None;
    assert_eq!(
        status(&c, &s, now),
        Status::Review,
        "unknown is not working"
    );
    let mut gone = s.clone();
    gone.current = false;
    c.background = Some(Background {
        agents: 1,
        ..Default::default()
    });
    assert_eq!(
        status(&c, &gone, now),
        Status::Review,
        "no live process, nothing runs"
    );
}
#[test]
fn a_stopped_turn_waiting_on_a_job_says_which_and_a_server_counts_apart() {
    let mut c = Content {
        turn: Turn::Completed,
        background: Some(Background {
            shells: 3,
            jobs: 1,
            ..Default::default()
        }),
        shells: vec![
            ShellTask {
                id: "a".into(),
                description: "Start the Go relay".into(),
                job: false,
            },
            ShellTask {
                id: "b".into(),
                description: "Run needle-in-haystack context tests on Pulse".into(),
                job: true,
            },
        ],
        ..Default::default()
    };
    assert_eq!(
        c.doing().as_deref(),
        Some("等后台任务 Run needle-in-haystack context tests on …")
    );
    assert_eq!(
        c.background.unwrap().describe().as_deref(),
        Some("后台仍有任务 1 个、shell 2 个")
    );
    assert_eq!(c.here().next, "等后台任务跑完，Agent 会接着做");
    c.shells[1].job = false;
    c.background = Some(Background {
        shells: 3,
        ..Default::default()
    });
    assert_eq!(c.doing(), None, "servers alone: nothing it is doing");
}
