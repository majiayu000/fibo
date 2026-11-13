use super::*;
use fibo_core::content::Content;

fn record(kind: &str, exit_code: Option<i32>, at: u64) -> Evidence {
    Evidence {
        kind: kind.into(),
        summary: kind.into(),
        exit_code,
        at,
        source: Default::default(),
    }
}
#[test]
fn status_follows_outcome_sessions_and_evidence() {
    let mut w = super::super::tests::fixture();
    let mut task = Task {
        id: "task".into(),
        space: "space".into(),
        title: "标签命名".into(),
        description: String::new(),
        acceptance: vec![AcceptanceCriterion {
            id: "c".into(),
            description: "测试通过".into(),
            evidence: vec![],
        }],
        agent: Agent::Codex,
    };
    w.tasks.push(task.clone());
    assert_eq!(task_status(&w, &task), TaskStatus::NotStarted);
    w.agent_sessions[0].task = Some(task.id.clone());
    // Every session it had is marked done, and the todo is not: it says so instead
    // of staying 进行中 for good.
    assert_eq!(task_status(&w, &task), TaskStatus::Settled);
    w.agent_sessions[0].outcome = Outcome::Open;
    assert_eq!(task_status(&w, &task), TaskStatus::Working);
    let sid = w.agent_sessions[0].id.clone();
    w.content.insert(
        sid.clone(),
        Content {
            evidence: vec![
                record("command", Some(0), 1),
                record("test", Some(101), 2),
                record("file", Some(0), 3),
            ],
            ..Default::default()
        },
    );
    // Plain commands are never offered; the newest record comes first.
    let offered = attachable(&w, &task.id, None);
    assert_eq!(
        offered.iter().map(|e| e.record.at).collect::<Vec<_>>(),
        [3, 2]
    );
    // A failed test on the criterion is an attempt: still 进行中.
    task.acceptance[0].evidence.push(offered[1].clone());
    assert_eq!(task_status(&w, &task), TaskStatus::Working);
    task.acceptance[0].evidence.push(offered[0].clone());
    assert_eq!(task_status(&w, &task), TaskStatus::Proven);
    assert!(attachable(&w, &task.id, Some(&task.acceptance[0])).is_empty());
    w.task_outcomes.insert(task.id.clone(), Outcome::Completed);
    assert_eq!(task_status(&w, &task), TaskStatus::Completed);
}
