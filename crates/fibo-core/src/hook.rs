//! What runs when a session changes state or a command in a terminal finishes
//! (`hooks.json`): a shell command or a webhook, handed the event as JSON. Hooks only hear about sessions; they cannot change one, and a
//! slow or failing hook never holds up the host: each runs off its thread under a timeout.
use crate::agent::Loaded;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The `hooks.json` format's major version.
pub const API: u32 = 1;
/// The longest a hook may take before it is stopped.
pub const MAX_TIMEOUT_MS: u64 = 30_000;

/// A session entering a state, once per turn (and once per dialog for `waiting`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    /// A turn began.
    Started,
    /// The Agent asks you something.
    Waiting,
    Error,
    /// The turn finished and is yours to check.
    Done,
    /// A command typed at a shell prompt exited 0, as the shell marked it (OSC 133).
    #[serde(rename = "command_done")]
    CommandDone,
    /// A command typed at a shell prompt exited non-zero.
    #[serde(rename = "command_failed")]
    CommandFailed,
}
impl Event {
    pub fn is_command(self) -> bool {
        matches!(self, Self::CommandDone | Self::CommandFailed)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Waiting => "waiting",
            Self::Error => "error",
            Self::Done => "done",
            Self::CommandDone => "command_done",
            Self::CommandFailed => "command_failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub id: String,
    pub on: Vec<Event>,
    /// Run with `sh -c`; the event arrives as JSON on stdin and as `MOORING_*` variables.
    #[serde(default)]
    pub command: Option<String>,
    /// POSTed the event as JSON.
    #[serde(default)]
    pub webhook: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Command events only: commands that took less than this are not reported.
    #[serde(default)]
    pub min_ms: u64,
    #[serde(skip)]
    pub file: PathBuf,
    /// Events named in `on` that this build does not know (a newer Fibo's): skipped.
    #[serde(skip)]
    pub skipped: Vec<String>,
}
fn default_timeout() -> u64 {
    5_000
}

/// The `on` names a build knows and the ones it does not.
fn known_events(names: Vec<serde_json::Value>) -> (Vec<serde_json::Value>, Vec<String>) {
    let (mut known, mut unknown) = (Vec::new(), Vec::new());
    for name in names {
        if serde_json::from_value::<Event>(name.clone()).is_ok() {
            known.push(name);
        } else {
            unknown.push(
                name.as_str()
                    .map_or_else(|| name.to_string(), str::to_owned),
            );
        }
    }
    (known, unknown)
}

/// What a hook is told. Only the session's card: never the transcript. A command event
/// has no session, Agent or title (all empty) and tells the terminal, the exit code and how
/// long the command ran instead; never the command line, which may carry a secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Payload {
    pub event: Event,
    pub session: String,
    pub agent: String,
    pub title: String,
    pub cwd: String,
    pub space: Option<String>,
    /// The question, the error, or the last reply's first line.
    pub detail: String,
    pub at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}
/// One line of `fibo events`: a hook's payload under the hook's event name, or one
/// only subscribers see: `approval` / `approval_gone` (the question in `detail`),
/// `terminal_started` / `terminal_exited`, and `lagged` (events were dropped; read the
/// snapshot again). Card fields only, never the conversation's words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub event: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(default)]
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}
impl From<Payload> for Notice {
    fn from(p: Payload) -> Self {
        Self {
            event: p.event.name().to_owned(),
            session: p.session,
            agent: p.agent,
            title: p.title,
            cwd: p.cwd,
            space: p.space,
            detail: p.detail,
            at: p.at,
            terminal: p.terminal,
            exit_code: p.exit_code,
            duration_ms: p.duration_ms,
        }
    }
}
impl Notice {
    pub fn named(event: &str, at: u64) -> Self {
        Self {
            event: event.to_owned(),
            session: String::new(),
            agent: String::new(),
            title: String::new(),
            cwd: String::new(),
            space: None,
            detail: String::new(),
            at,
            terminal: None,
            exit_code: None,
            duration_ms: None,
        }
    }
}
impl Payload {
    /// The same facts as environment variables, for one-line shell hooks.
    pub fn env(&self) -> Vec<(String, String)> {
        let vars: Vec<(&str, String)> = vec![
            ("MOORING_EVENT", self.event.name().into()),
            ("MOORING_SESSION", self.session.clone()),
            ("MOORING_AGENT", self.agent.clone()),
            ("MOORING_TITLE", self.title.clone()),
            ("MOORING_CWD", self.cwd.clone()),
            ("MOORING_SPACE", self.space.clone().unwrap_or_default()),
            ("MOORING_DETAIL", self.detail.clone()),
            (
                "MOORING_TERMINAL",
                self.terminal.clone().unwrap_or_default(),
            ),
            (
                "MOORING_EXIT_CODE",
                self.exit_code.map(|c| c.to_string()).unwrap_or_default(),
            ),
            (
                "MOORING_DURATION_MS",
                self.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
            ),
        ];
        // Plugins written for Keepline read the old names.
        crate::legacy::with_old_names(vars.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    api: u32,
    hooks: Vec<Hook>,
}

fn read(file: &Path) -> Result<Vec<Hook>, String> {
    let bytes = std::fs::read(file).map_err(|e| t!("读不了：{e}", e))?;
    // An event this build does not know is skipped, not the whole file: a plugin written for
    // a newer Fibo keeps working for the events both know.
    let mut value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| crate::plugin::bad_format(&e))?;
    let mut skipped: Vec<Vec<String>> = Vec::new();
    let mut empty_after_skip: Vec<bool> = Vec::new();
    for h in value["hooks"].as_array_mut().into_iter().flatten() {
        let Some(on) = h.get_mut("on").and_then(|o| o.as_array_mut()) else {
            skipped.push(vec![]);
            empty_after_skip.push(false);
            continue;
        };
        let (known, unknown) = known_events(std::mem::take(on));
        empty_after_skip.push(known.is_empty() && !unknown.is_empty());
        *on = known;
        skipped.push(unknown);
    }
    let f: File = serde_json::from_value(value).map_err(|e| crate::plugin::bad_format(&e))?;
    if f.api != API {
        return Err(t!(
            "hooks 格式是 api {got}，这个版本只认 api {API}",
            got = f.api,
            API
        ));
    }
    if f.hooks.is_empty() {
        return Err(t!("hooks 至少写一条").into());
    }
    let mut hooks = f.hooks;
    for i in 0..hooks.len() {
        let (before, rest) = hooks.split_at_mut(i);
        let h = &mut rest[0];
        if !crate::plugin::valid_id(&h.id) {
            return Err(t!(
                "hook id \"{id}\" 只能用小写字母、数字和 -，不超过 40 个字",
                id = h.id
            ));
        }
        // Inside its own plugin only: nothing stores a hook's id, so two plugins' hooks may
        // share a name, and installing one can never break another.
        if before.iter().any(|t| t.id == h.id) {
            return Err(t!("hook id \"{id}\" 重名", id = h.id));
        }
        if h.on.is_empty() && !empty_after_skip[i] {
            return Err(t!("hook {id} 的 on 至少写一个事件", id = h.id));
        }
        h.skipped = skipped[i].clone();
        match (&h.command, &h.webhook) {
            (Some(c), None) if !c.trim().is_empty() => {}
            (None, Some(u)) if u.starts_with("https://") || u.starts_with("http://") => {}
            (None, Some(_)) => {
                return Err(t!("hook {id} 的 webhook 要以 http(s):// 开头", id = h.id));
            }
            _ => return Err(t!("hook {id} 要写 command 或 webhook 其中一个", id = h.id)),
        }
        if !(100..=MAX_TIMEOUT_MS).contains(&h.timeout_ms) {
            return Err(t!(
                "hook {id} 的 timeout_ms 要在 100 到 {MAX_TIMEOUT_MS} 之间",
                id = h.id,
                MAX_TIMEOUT_MS
            ));
        }
        if h.min_ms > 0 && !h.on.iter().any(|e| e.is_command()) {
            return Err(t!(
                "hook {id} 的 min_ms 只对 command_done、command_failed 有用",
                id = h.id
            ));
        }
        h.file = file.to_owned();
    }
    Ok(hooks)
}

pub fn load(files: Vec<PathBuf>) -> (Vec<Hook>, Vec<Loaded>) {
    let mut hooks: Vec<Hook> = Vec::new();
    let mut report = Vec::new();
    for file in files {
        let result = read(&file);
        report.push(Loaded {
            file: file.clone(),
            result: result
                .as_ref()
                .map(|found| {
                    found
                        .iter()
                        .map(|h| {
                            let on: Vec<_> = h.on.iter().map(|e| e.name()).collect();
                            if h.skipped.is_empty() {
                                t!("{id}（{on}）", id = h.id, on = on.join("/"))
                            } else {
                                t!(
                                    "{id}（{on}；这个版本不认识 {skipped}，跳过了）",
                                    id = h.id,
                                    on = on.join("/"),
                                    skipped = h.skipped.join(t!("、"))
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(t!("、"))
                })
                .map_err(Clone::clone),
        });
        if let Ok(found) = result {
            hooks.extend(found);
        }
    }
    (hooks, report)
}

static HOOKS: crate::plugin::Slot<(Vec<Hook>, Vec<Loaded>)> = crate::plugin::Slot::new();

/// Loads the installed plugins' hooks once (after `plugin::install`).
pub fn install() {
    HOOKS.get_or_init(|| load(crate::plugin::files(crate::plugin::Kind::Hooks)));
}
/// Loads them again after plugins changed (`plugin::reload`).
pub(crate) fn reload() {
    HOOKS.set(load(crate::plugin::files(crate::plugin::Kind::Hooks)));
}
pub fn all() -> &'static [Hook] {
    HOOKS.get().map_or(&[], |(h, _)| h.as_slice())
}
pub fn report() -> &'static [Loaded] {
    HOOKS.get().map_or(&[], |(_, r)| r.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_need_one_target_a_known_event_and_a_sane_timeout() {
        let d = std::env::temp_dir().join(format!("kl-hooks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let write = |name: &str, body: &str| {
            let p = d.join(name);
            std::fs::write(&p, body).unwrap();
            p
        };
        let good = write(
            "a.json",
            r#"{"api":1,"hooks":[{"id":"say","on":["waiting","error"],"command":"say \"$MOORING_TITLE\""},{"id":"slack","on":["done"],"webhook":"https://hooks.example.com/x","timeout_ms":2000}]}"#,
        );
        let both = write(
            "b.json",
            r#"{"api":1,"hooks":[{"id":"x","on":["done"],"command":"true","webhook":"https://a"}]}"#,
        );
        let event = write(
            "c.json",
            r#"{"api":1,"hooks":[{"id":"y","on":["finished"],"command":"true"}]}"#,
        );
        let slow = write(
            "d.json",
            r#"{"api":1,"hooks":[{"id":"z","on":["done"],"command":"true","timeout_ms":600000}]}"#,
        );
        let dup = write(
            "e.json",
            r#"{"api":1,"hooks":[{"id":"x","on":["done"],"command":"true"},{"id":"x","on":["done"],"command":"true"}]}"#,
        );
        let long = write(
            "f.json",
            r#"{"api":1,"hooks":[{"id":"long","on":["command_done","command_failed"],"command":"true","min_ms":10000}]}"#,
        );
        let stray = write(
            "g.json",
            r#"{"api":1,"hooks":[{"id":"stray","on":["done"],"command":"true","min_ms":10}]}"#,
        );
        let (hooks, report) = load(vec![good, both, event, slow, dup, long, stray]);
        // The unknown event costs that hook its event; see the test below.
        assert_eq!(hooks.len(), 4);
        assert_eq!(hooks[3].min_ms, 10_000);
        assert_eq!(
            report[5].result,
            Ok("long（command_done/command_failed）".into())
        );
        assert!(report[6].result.as_ref().unwrap_err().contains("min_ms"));
        assert_eq!(hooks[0].timeout_ms, 5_000);
        assert_eq!(
            report[0].result,
            Ok("say（waiting/error）、slack（done）".into())
        );
        assert!(report[1].result.as_ref().unwrap_err().contains("其中一个"));
        assert!(report[2].result.as_ref().unwrap().contains("跳过了"));
        assert!(
            report[3]
                .result
                .as_ref()
                .unwrap_err()
                .contains("timeout_ms")
        );
        assert!(report[4].result.as_ref().unwrap_err().contains("重名"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A new event name costs a plugin that event, not the whole file.
    #[test]
    fn an_event_from_a_newer_mooring_is_skipped_not_fatal() {
        let d = std::env::temp_dir().join(format!("kl-hook-future-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("hooks.json");
        std::fs::write(
            &f,
            r#"{"api":1,"hooks":[{"id":"a","on":["done","subagent_done"],"command":"true"},
                {"id":"b","on":["only_new"],"command":"true"}]}"#,
        )
        .unwrap();
        let (hooks, report) = load(vec![f.clone()]);
        assert_eq!(hooks[0].on, [Event::Done]);
        assert!(hooks[1].on.is_empty(), "never fires");
        let said = report[0].result.as_ref().unwrap();
        assert!(
            said.contains("subagent_done") && said.contains("only_new"),
            "{said}"
        );
        std::fs::write(
            &f,
            r#"{"api":1,"hooks":[{"id":"a","on":[],"command":"true"}]}"#,
        )
        .unwrap();
        assert!(
            load(vec![f.clone()]).1[0]
                .result
                .as_ref()
                .unwrap_err()
                .contains("至少")
        );
        std::fs::write(
            &f,
            r#"{"api":1,"hooks":[{"id":"a","on":["done"],"retry":3,"command":"true"}]}"#,
        )
        .unwrap();
        assert!(
            load(vec![f]).1[0]
                .result
                .as_ref()
                .unwrap_err()
                .contains("更新 Fibo")
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
