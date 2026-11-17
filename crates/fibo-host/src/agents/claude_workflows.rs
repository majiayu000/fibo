//! Claude's parent-owned workflows. Read the journal, never execute its script.
use crate::content::{text, Parser};
use fibo_core::content::{Subagent, Workflow, WorkflowStep};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(super) fn parent(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let dir = path.parent()?;
    let session = if dir.file_name()? == "workflows" && name.ends_with(".json") {
        dir.parent()?
    } else if dir.parent()?.file_name()? == "workflows"
        && dir.parent()?.parent()?.file_name()? == "subagents"
        && (name == "journal.jsonl" || name.starts_with("agent-") && name.ends_with(".meta.json"))
    {
        dir.parent()?.parent()?.parent()?
    } else {
        return None;
    };
    let mut parent = session.as_os_str().to_owned();
    parent.push(".jsonl");
    Some(parent.into())
}

/// Kept with the normal hook receipt; no new status rule or watcher.
pub(crate) fn tasks(v: &Value, ended: &[&str]) -> Option<Vec<String>> {
    Some(
        v["background_tasks"]
            .as_array()?
            .iter()
            .filter(|t| matches!(t["type"].as_str(), Some("workflow" | "local_workflow")))
            .filter(|t| {
                !ended.contains(&t["status"].as_str().unwrap_or("").to_lowercase().as_str())
            })
            .filter_map(|t| t["id"].as_str().map(str::to_owned))
            .collect(),
    )
}

/// Successful TaskStop receipts and notifications are exact task-id evidence. Merely
/// asking to stop (or quoting it in prose) does not close a workflow.
pub(super) fn note_parent(parser: &mut Parser, v: &Value) {
    if v["type"] == "attachment" && v["attachment"]["commandMode"] == "task-notification" {
        parser
            .card
            .note_task_notification(&text(&v["attachment"]["prompt"]));
    }
    let Some(blocks) = v["message"]["content"].as_array() else {
        return;
    };
    for block in blocks
        .iter()
        .filter(|b| b["type"] == "tool_result" && b["is_error"] != true)
    {
        let said = text(&block["content"]);
        if said.starts_with("Workflow launched in background. Task ID: ") {
            if let Some(id) = said
                .lines()
                .next()
                .and_then(|s| s.split_once("Task ID: ").map(|(_, id)| id.trim()))
            {
                parser.card.finished_subagents.retain(|old| old != id);
                parser.card.stopped_subagents.retain(|old| old != id);
                // A launch after a Stop is fresh evidence; the old registry cannot end it.
                if let Some((_, tasks)) = &mut parser.card.workflow_tasks {
                    if !tasks.iter().any(|old| old == id) {
                        tasks.push(id.to_owned());
                    }
                }
            }
        }
        let Ok(stop) = serde_json::from_str::<Value>(&said) else {
            continue;
        };
        if stop["task_type"] == "local_workflow"
            && stop["message"]
                .as_str()
                .is_some_and(|s| s.starts_with("Successfully stopped task:"))
        {
            if let Some(id) = stop["task_id"].as_str() {
                if !parser.card.stopped_subagents.iter().any(|s| s == id) {
                    parser.card.stopped_subagents.push(id.to_owned());
                }
            }
        }
    }
}

fn json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
fn string(v: &Value) -> String {
    v.as_str().unwrap_or("").to_owned()
}
fn seconds(v: &Value) -> Option<u64> {
    v.as_str()?
        .parse::<jiff::Timestamp>()
        .ok()?
        .as_second()
        .try_into()
        .ok()
}
fn modified(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|t| t.as_secs())
}

pub(super) fn read(transcript: &Path) -> Vec<Subagent> {
    let session = transcript.with_extension("");
    let root = session.join("subagents/workflows");
    let mut runs = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        runs.extend(
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name()),
        );
    }
    if let Ok(entries) = std::fs::read_dir(session.join("workflows")) {
        runs.extend(entries.flatten().filter_map(|e| {
            (e.path().extension()? == "json").then(|| e.path().file_stem().unwrap().to_owned())
        }));
    }
    let mut out = Vec::new();
    for run in runs {
        let dir = root.join(run);
        let Some(run) = dir.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let snapshot =
            json(&session.join("workflows").join(format!("{run}.json"))).unwrap_or_default();
        let mut agents: HashMap<_, _> = super::claude::subagents(&dir)
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();
        let started = snapshot["startTime"]
            .as_u64()
            .map(|t| t / 1000)
            .or_else(|| agents.values().map(|s| s.started).min())
            .or_else(|| modified(&dir))
            .unwrap_or(0);
        let mut workflow = Workflow {
            phases: snapshot["phases"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["title"].as_str().map(str::to_owned))
                .collect(),
            detailed: true,
            ..Default::default()
        };
        let metas_count = agents.len();
        let journal = dir.join("journal.jsonl");
        match std::fs::read_to_string(&journal) {
            Ok(log) => {
                let mut by_key = HashMap::<String, usize>::new();
                let mut attempts = 0;
                for line in log.lines() {
                    let Ok(v) = serde_json::from_str::<Value>(line) else {
                        workflow.detailed = false;
                        continue;
                    };
                    match v["type"].as_str() {
                        Some("launched") => (),
                        Some("started") => {
                            let (Some(key), Some(id)) = (v["key"].as_str(), v["agentId"].as_str())
                            else {
                                workflow.detailed = false;
                                continue;
                            };
                            attempts += 1;
                            let mut agent = agents.remove(id).unwrap_or_else(|| Subagent {
                                id: id.into(),
                                started,
                                ..Default::default()
                            });
                            agent.name = v["label"].as_str().unwrap_or(&agent.name).to_owned();
                            // Journal results belong to this attempt; stale meta cannot
                            // finish its replacement, even when the key is unchanged.
                            agent.ended = None;
                            agent.stopped = false;
                            let step = WorkflowStep {
                                key: key.into(),
                                phase: string(&v["phase"]),
                                agent,
                            };
                            workflow.phase = step.phase.clone();
                            if let Some(i) = by_key.get(key) {
                                workflow.steps[*i] = step;
                            } else {
                                by_key.insert(key.into(), workflow.steps.len());
                                workflow.steps.push(step);
                            }
                        }
                        Some("result") => {
                            if let Some(i) = v["key"].as_str().and_then(|key| by_key.get(key)) {
                                let s = &mut workflow.steps[*i].agent;
                                if v["agentId"].as_str() == Some(s.id.as_str()) {
                                    s.ended = Some(
                                        modified(&dir.join(format!("agent-{}.jsonl", s.id)))
                                            .unwrap_or(s.started)
                                            .max(s.started),
                                    );
                                }
                            } else {
                                workflow.detailed = false;
                            }
                        }
                        _ => workflow.detailed = false,
                    }
                }
                if attempts == 0 {
                    workflow.detailed = false;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut metas: Vec<_> = agents.into_values().collect();
                metas.sort_by_key(|s| s.started);
                for agent in metas {
                    let meta = json(&dir.join(format!("agent-{}.meta.json", agent.id)))
                        .unwrap_or_default();
                    let phase = string(&meta["workflowPhase"]);
                    workflow.phase = phase.clone();
                    workflow.steps.push(WorkflowStep {
                        key: agent.id.clone(),
                        phase,
                        agent,
                    });
                }
            }
            Err(_) => workflow.detailed = false,
        }
        if workflow.steps.is_empty() {
            workflow.detailed = false;
        }
        workflow.total = if workflow.detailed {
            workflow.steps.len()
        } else {
            snapshot["agentCount"]
                .as_u64()
                .map(|n| n as usize)
                .unwrap_or(metas_count.max(workflow.steps.len()))
        };
        // Include phases seen in older records whose snapshot had no phase declaration.
        for step in &workflow.steps {
            if !workflow.phases.contains(&step.phase) {
                workflow.phases.push(step.phase.clone());
            }
        }
        let status = snapshot["status"].as_str().unwrap_or("");
        let stopped = matches!(
            status,
            "killed" | "stopped" | "failed" | "cancelled" | "aborted"
        );
        let ended = (stopped || matches!(status, "completed" | "complete" | "done")).then(|| {
            seconds(&snapshot["timestamp"])
                .unwrap_or(started + snapshot["durationMs"].as_u64().unwrap_or(0) / 1000)
        });
        out.push(Subagent {
            id: snapshot["taskId"].as_str().unwrap_or(run).to_owned(),
            name: snapshot["workflowName"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(run)
                .to_owned(),
            started,
            ended,
            stopped,
            foreground: false,
            workflow: Some(workflow),
        });
    }
    out
}
