//! Workflow source shapes through the real adapter, parent parser and durable index.
use fibo_core::{
    content::{Content, Turn, WorkflowRun},
    model::Agent,
};
use fibo_host::{content::Parser, log_index::Index};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("fibo-workflow-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn log(&self) -> PathBuf {
        self.0.join("p/parent.jsonl")
    }
    fn dir(&self) -> PathBuf {
        self.log()
            .with_extension("")
            .join("subagents/workflows/wf_test")
    }
    fn prepare(&self) {
        fs::create_dir_all(self.dir()).unwrap();
        fs::write(self.log(), format!("{}\n", json!({"type":"user","sessionId":"parent","cwd":"/repo", "timestamp":10,"message":{"content":"检查工作流"}}))).unwrap();
        self.snapshot("running");
        for (id, phase, label) in [
            ("old", "Review", "review:a"),
            ("new", "Review", "review:a"),
            ("done", "Fix", "fix:b"),
        ] {
            fs::write(self.dir().join(format!("agent-{id}.jsonl")), "{}\n").unwrap();
            fs::write(
                self.dir().join(format!("agent-{id}.meta.json")),
                json!({"description":label,"workflowPhase":phase,"requestShape":"foreground"})
                    .to_string(),
            )
            .unwrap();
        }
        for v in [
            json!({"type":"launched"}),
            json!({"type":"started","key":"a","agentId":"old","label":"review:a","phase":"Review"}),
            json!({"type":"result","key":"a","agentId":"old"}),
            json!({"type":"started","key":"a","agentId":"new","label":"review:a","phase":"Review"}),
            // A delayed result from the replaced attempt must not finish the new one.
            json!({"type":"result","key":"a","agentId":"old"}),
            json!({"type":"started","key":"b","agentId":"done","label":"fix:b","phase":"Fix"}),
            json!({"type":"result","key":"b","agentId":"done"}),
        ] {
            append(&self.dir().join("journal.jsonl"), v);
        }
    }
    fn snapshot(&self, status: &str) {
        let p = self.log().with_extension("").join("workflows/wf_test.json");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, json!({"runId":"wf_test","taskId":"w1","workflowName":"queue","startTime":10000,"durationMs":20000,"status":status,"phases":[{"title":"Review"},{"title":"Fix"}]}).to_string()).unwrap();
    }
    fn read(&self) -> Content {
        let all = fibo_host::agent_adapter(Agent::Claude)
            .subagents(&self.log())
            .unwrap();
        let mut c = Content {
            turn: Turn::Running,
            turn_started: 10,
            updated: 30,
            ..Default::default()
        };
        c.keep_subagents(all);
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn append(p: &Path, v: Value) {
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .unwrap(),
        "{v}"
    )
    .unwrap();
}

#[test]
fn workflow_retries_keep_only_the_latest_attempt_and_its_own_result() {
    let f = Fixture::new();
    f.prepare();
    let c = f.read();
    assert_eq!(c.subagents.len(), 1);
    let run = &c.subagents[0];
    let w = run.workflow.as_ref().unwrap();
    assert_eq!(
        (run.id.as_str(), run.name.as_str(), w.phase.as_str()),
        ("w1", "queue", "Fix")
    );
    assert_eq!(w.phases, ["Review", "Fix"]);
    assert_eq!(w.steps.len(), 2);
    assert_eq!(w.steps[0].agent.id, "new");
    assert!(w.steps[0].agent.running());
    assert_eq!(w.completed(true), 1);
    assert!(run.workflow_summary(true).unwrap().contains("1/2"));
    // Interrupted in a later parent turn: keep where it stopped, not just this turn's starts.
    f.snapshot("killed");
    let mut c = f.read();
    c.turn_started = 25;
    c.keep_subagents(
        fibo_host::agent_adapter(Agent::Claude)
            .subagents(&f.log())
            .unwrap(),
    );
    let run = &c.subagents[0];
    assert!(run.stopped && !run.running());
    let w = run.workflow.as_ref().unwrap();
    assert!(w.steps[0].agent.stopped);
    assert!(!w.steps[1].agent.stopped);
    assert_eq!(w.completed(false), 1);
    println!("{}", run.workflow_summary(false).unwrap());
}

#[test]
fn workflow_legacy_meta_and_changed_journal_have_explicit_fallbacks() {
    let f = Fixture::new();
    f.prepare();
    fs::remove_file(f.dir().join("journal.jsonl")).unwrap();
    let c = f.read();
    let run = &c.subagents[0];
    assert!(run.workflow.as_ref().unwrap().detailed);
    assert_eq!(run.workflow.as_ref().unwrap().completed(false), 3);
    append(
        &f.dir().join("journal.jsonl"),
        json!({"type":"new-format","steps":3}),
    );
    let c = f.read();
    let run = &c.subagents[0];
    assert!(!run.workflow.as_ref().unwrap().detailed);
    assert!(!run.workflow_summary(true).unwrap().contains("阶段"));
}

#[test]
fn workflow_changed_directory_keeps_only_a_snapshot_summary() {
    let f = Fixture::new();
    f.prepare();
    fs::remove_dir_all(f.dir()).unwrap();
    let snapshot = f.log().with_extension("").join("workflows/wf_test.json");
    let mut v: Value = serde_json::from_slice(&fs::read(&snapshot).unwrap()).unwrap();
    v["agentCount"] = json!(6);
    fs::write(snapshot, v.to_string()).unwrap();
    let c = f.read();
    let run = &c.subagents[0];
    let w = run.workflow.as_ref().unwrap();
    assert!(!w.detailed && w.steps.is_empty());
    assert_eq!(w.total, 6);
    assert!(run.workflow_summary(true).unwrap().contains("6 步"));
}

#[test]
fn workflow_journal_and_snapshot_refresh_parent_without_parent_log_writes_and_survive_restart() {
    let f = Fixture::new();
    f.prepare();
    let key = f.log().to_string_lossy().into_owned();
    let mut index = Index::open(&f.0, vec![(Agent::Claude, f.0.join("p"))]).unwrap();
    index.recent_days = 100000;
    index.initialize().unwrap();
    index.update_many([f.log()], 1).unwrap();
    assert_eq!(
        index.card(&key).unwrap().unwrap().subagents[0]
            .workflow
            .as_ref()
            .unwrap()
            .completed(true),
        1
    );
    let before = fs::read(f.log()).unwrap();
    let journal = f.dir().join("journal.jsonl");
    append(&journal, json!({"type":"result","key":"a","agentId":"new"}));
    let changed = index.update_many([journal], 2).unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].path, key);
    assert_eq!(
        index.card(&key).unwrap().unwrap().subagents[0]
            .workflow
            .as_ref()
            .unwrap()
            .completed(true),
        2
    );
    f.snapshot("killed");
    index
        .update_many(
            [f.log().with_extension("").join("workflows/wf_test.json")],
            3,
        )
        .unwrap();
    assert!(index.card(&key).unwrap().unwrap().subagents[0].stopped);
    assert_eq!(before, fs::read(f.log()).unwrap());
    index.flush().unwrap();
    drop(index);
    let mut index = Index::open(&f.0, vec![(Agent::Claude, f.0.join("p"))]).unwrap();
    index.recent_days = 100000;
    index.initialize().unwrap();
    assert!(index.card(&key).unwrap().unwrap().subagents[0].stopped);
    for file in ["journal.jsonl", "agent-new.meta.json"] {
        assert_eq!(
            fibo_host::agent_adapter(Agent::Claude).subagent_parent(&f.dir().join(file)),
            Some(f.log())
        );
    }
}

#[test]
fn workflow_notifications_taskstop_and_exact_stop_registry_end_only_the_named_run() {
    let f = Fixture::new();
    f.prepare();
    let mut p = Parser::default();
    p.card = f.read();
    let stop = json!({"type":"user","timestamp":30,"message":{"content":[{"type":"tool_result","content":json!({"task_type":"local_workflow","task_id":"w1","message":"Successfully stopped task: w1"}).to_string()}]}});
    p.add(
        Agent::Claude,
        &serde_json::to_vec(&stop).unwrap(),
        Default::default(),
    );
    p.card.keep_subagents(f.read().subagents);
    assert!(p.card.subagents[0].stopped);
    let mut c = f.read();
    c.note_task_notification("<task-notification><task-id>w1</task-id><status>completed</status>");
    c.keep_subagents(f.read().subagents);
    assert!(!c.subagents[0].running() && !c.subagents[0].stopped);
    assert_eq!(
        c.subagents[0].workflow.as_ref().unwrap().completed(false),
        1,
        "a run notification does not invent missing journal results"
    );
    let facts = fibo_host::content::hook_facts(
        &json!({"background_tasks":[{"type":"agent","id":"ordinary","status":"running"}]}),
    );
    let mut p = Parser::default();
    p.card = f.read();
    p.hook("Stop", &facts, 40, Default::default());
    assert!(
        p.card.subagents[0].stopped,
        "the ordinary agent must not keep a disappeared workflow alive"
    );
}

#[test]
fn workflow_resume_clears_an_earlier_stop_receipt_and_registry() {
    let f = Fixture::new();
    f.prepare();
    let mut p = Parser::default();
    p.card = f.read();
    p.card.stopped_subagents.push("w1".into());
    p.card.finished_subagents.push("w1".into());
    p.card.workflow_tasks = Some((40, vec![]));
    p.add(Agent::Claude, &serde_json::to_vec(&json!({"type":"user","timestamp":50,
        "message":{"content":[{"type":"tool_result","content":"Workflow launched in background. Task ID: w1\nRun ID: wf_test"}]}})).unwrap(), Default::default());
    p.card.keep_subagents(f.read().subagents);
    assert!(p.card.subagents[0].running());
    assert!(!p.card.subagents[0].stopped);
}

#[test]
#[ignore = "read-only real source replay: set FIBO_REPLAY_CLAUDE_WORKFLOW to parent.jsonl"]
fn replay_loom_workflow_read_only() {
    let log = PathBuf::from(std::env::var("FIBO_REPLAY_CLAUDE_WORKFLOW").unwrap());
    let mut p = Parser::default();
    use std::io::BufRead;
    for line in std::io::BufReader::new(fs::File::open(&log).unwrap()).lines() {
        p.add(Agent::Claude, line.unwrap().as_bytes(), Default::default());
    }
    let all = fibo_host::agent_adapter(Agent::Claude)
        .subagents(&log)
        .unwrap();
    let mut c = p.card;
    c.keep_subagents(all);
    let run = c
        .subagents
        .iter()
        .find(|s| s.name == "loom-pr-queue")
        .unwrap();
    let w = run.workflow.as_ref().unwrap();
    assert_eq!(w.steps.len(), 6);
    assert_eq!(w.completed(false), 2);
    assert!(run.stopped);
    assert_eq!(w.phase, "Verify");
    println!(
        "{}; phases={:?}; steps={:?}",
        run.workflow_summary(false).unwrap(),
        w.phases,
        w.steps
            .iter()
            .map(|s| (&s.phase, &s.agent.name, s.agent.stopped))
            .collect::<Vec<_>>()
    );
}

/// Claude Code writes `workflows/<run>.json` only once a run ends: while it runs, the run
/// is known by its directory, and its task id and name only from the parent's launch.
#[test]
fn a_running_workflow_without_its_snapshot_keeps_running_past_the_parent_stop() {
    let f = Fixture::new();
    f.prepare();
    fs::remove_file(f.log().with_extension("").join("workflows/wf_test.json")).unwrap();
    // Without a snapshot it started when its files were written: now.
    let at = jiff::Timestamp::now().as_second() as u64 + 60;
    let facts = fibo_host::content::hook_facts(
        &json!({"background_tasks":[{"type":"local_workflow","id":"w1","status":"running"}]}),
    );
    // The launch receipt is in the part of the log that was read.
    let mut p = Parser::default();
    p.card = f.read();
    let launch = json!({"type":"user","timestamp":12,"message":{"content":[{"type":"tool_result",
        "content":"Workflow launched in background. Task ID: w1\nSummary: s\nTranscript dir: /p/parent/subagents/workflows/wf_test"}]},
        "toolUseResult":{"status":"async_launched","taskId":"w1","taskType":"local_workflow","workflowName":"queue","runId":"wf_test"}});
    p.add(
        Agent::Claude,
        &serde_json::to_vec(&launch).unwrap(),
        Default::default(),
    );
    p.card.keep_subagents(f.read().subagents);
    let run = &p.card.subagents[0];
    assert_eq!((run.id.as_str(), run.name.as_str()), ("w1", "queue"));
    p.hook("Stop", &facts, at, Default::default());
    p.card.keep_subagents(f.read().subagents);
    let run = &p.card.subagents[0];
    assert!(
        run.running() && !run.stopped,
        "the registry names it: {run:?}"
    );
    // The launch sits in the unread middle of a long log: its task id is unknown, and a
    // registry that cannot name it says nothing about it.
    let mut p = Parser::default();
    p.card = f.read();
    p.hook("Stop", &facts, at, Default::default());
    p.card.keep_subagents(f.read().subagents);
    let run = &p.card.subagents[0];
    assert!(run.running() && !run.stopped, "unknown task id: {run:?}");
}
