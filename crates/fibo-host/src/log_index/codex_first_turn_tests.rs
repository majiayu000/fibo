//! The first prompt of a shared-daemon Codex conversation, bound to its pane by the Enter that
//! started its turn. The records are a real Codex 0.172 rollout (`fibo run codex` in the
//! v0.1.4 RC1 walk, 2026-11-24, `tests/fixtures/codex-0172-first-turn.jsonl`), its long
//! strings cut: the turn starts at Enter (15:19:45), then tools, instructions and AGENTS.md
//! are written, and the task only at 15:19:51, past the 3 s an Enter press may be from it.
use super::*;
use crate::recognition::{screen_bindings, screen_key, ScreenTerminal, ScreenThread};

const ROLLOUT: &str = include_str!("../../tests/fixtures/codex-0172-first-turn.jsonl");
const TASK: &str = "echo walk-r14x-sixth-no-update";
/// 2026-11-24T15:19:45Z, when the turn started and `fibo run` pressed Enter.
const ENTER: u64 = 1_795_533_585;

/// The fixture's lines, with its tools record padded to `tools` bytes of description: the
/// real one makes 212 KiB before the task (0.169: about 90 KiB, under the 128 KiB a first
/// read of a recent log takes from its end).
fn rollout(tools: usize) -> Vec<String> {
    (ROLLOUT.lines())
        .map(|line| {
            let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
            if v["payload"]["type"] == "additional_tools" {
                v["payload"]["tools"][0]["description"] = "x".repeat(tools).into();
            }
            v.to_string()
        })
        .collect()
}

#[test]
fn a_codex_0172_first_prompt_binds_by_the_enter_that_started_its_turn() {
    for tools in [0, 200 * 1024] {
        let root = std::env::temp_dir().join(format!("kl-172-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("logs")).unwrap();
        let root = root.canonicalize().unwrap();
        let log = root.join("logs/rollout.jsonl");
        let lines = rollout(tools);
        let task = lines.iter().position(|l| l.contains(TASK)).unwrap();
        let mut index = Index::open(&root, vec![(Agent::Codex, root.join("logs"))]).unwrap();
        index.initialize().unwrap();
        // First seen as the host saw it: everything up to the task, then the task.
        let mut file = fs::File::create(&log).unwrap();
        for (i, range) in [(0, task), (task, lines.len())].into_iter().enumerate() {
            for line in &lines[range.0..range.1] {
                writeln!(file, "{line}").unwrap();
            }
            index.update_many([log.clone()], i as u64 + 1).unwrap();
            index.flush().unwrap();
        }
        let path = log.to_str().unwrap();
        let (latest, at) = index.latest_user(path).unwrap().unwrap();
        assert_eq!((latest.as_str(), at), (TASK, ENTER + 6), "tools {tools}");
        let began = index.turn_began(path, at).unwrap();
        assert_eq!(
            began,
            Some(ENTER),
            "tools {tools}: the turn's start was dropped"
        );
        let transcript = index.cached().unwrap().remove(0);
        let thread = ScreenThread {
            transcript: &transcript,
            key: screen_key(&latest),
            at: Some(at),
            began,
            cwd: transcript.cwd.clone(),
            first: None,
        };
        let pane = ScreenTerminal {
            terminal: "pty".into(),
            runtime: Agent::Codex,
            cwd: Some(transcript.cwd.clone()),
            screen: String::new(),
            current: None,
            submits: vec![ENTER],
            launched: None,
        };
        let bound = screen_bindings(&[thread], &[pane]);
        assert_eq!(bound.len(), 1, "tools {tools}");
        let _ = fs::remove_dir_all(&root);
    }
}
