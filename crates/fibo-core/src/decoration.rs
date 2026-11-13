//! Short facts a plugin puts on each pane (`decorations.json`): a command run in the pane's
//! directory whose first line of output is shown in the pane's title bar or in its title's
//! hover card, like a tmux status segment. `node -v`, a kube context, a ticket number.
//!
//! The app never waits for one: each runs off the UI thread under its timeout and the last
//! line it printed stays until the next run, every `refresh_s` while the pane is drawn. A
//! command that exits non-zero or prints nothing shows nothing, so a script can opt out of
//! directories it does not apply to.
use crate::agent::Loaded;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The `decorations.json` format's major version.
pub const API: u32 = 1;
/// The longest a decoration may take before it is stopped.
pub const MAX_TIMEOUT_MS: u64 = 5_000;
/// Title bar room is scarce; the hover card has more.
pub const TITLE_CHARS: usize = 40;
pub const TIP_CHARS: usize = 120;

/// Where the line shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    /// In the pane's title bar, before the Agent and its time; the default.
    #[default]
    Title,
    /// A line of its own in the title's hover card.
    Tip,
}

/// The color of a title-bar line: the status colors the app already uses, so it follows
/// the user's `colors.json` and stays legible on either scheme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    /// The title bar's other facts; the default.
    #[default]
    Muted,
    /// As a finished session.
    Ok,
    /// As a session waiting for the user.
    Warn,
    /// As a session that failed.
    Error,
}

/// Which panes it is run for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum When {
    #[default]
    Always,
    /// Panes running an Agent session Fibo has bound.
    Agent,
    /// Panes with no Agent session: a plain shell.
    Shell,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decoration {
    pub id: String,
    /// Run with `sh -c` in the pane's directory; `MOORING_CWD`, `MOORING_AGENT` and
    /// `MOORING_SESSION` say which pane it is for, `MOORING_LAST_EXIT` and
    /// `MOORING_LAST_DURATION_MS` how its shell's last command ended (empty before one).
    pub command: String,
    #[serde(default)]
    pub place: Place,
    #[serde(default)]
    pub when: When,
    /// Its color in the title bar; a hover-card line is plain text.
    #[serde(default)]
    pub tone: Tone,
    /// How often it is run again while shown.
    #[serde(default = "default_refresh")]
    pub refresh_s: u64,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(skip)]
    pub file: PathBuf,
}
fn default_refresh() -> u64 {
    10
}
fn default_timeout() -> u64 {
    1_000
}

/// The pane a decoration is run for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pane {
    pub cwd: String,
    /// The Agent's name, when the pane runs a bound session.
    pub agent: Option<String>,
    pub session: Option<String>,
    /// The last command its shell marked finished; a new one runs the decoration again.
    pub last: Option<crate::protocol::LastCommand>,
}

impl Decoration {
    pub fn shown(&self, pane: &Pane) -> bool {
        match self.when {
            When::Always => true,
            When::Agent => pane.session.is_some(),
            When::Shell => pane.session.is_none(),
        }
    }
    pub fn refresh(&self) -> Duration {
        Duration::from_secs(self.refresh_s)
    }
    /// Runs it for `pane` (blocking, up to its timeout): the line to show, or why none.
    /// A non-zero exit is the script opting out, not a failure; a command that cannot
    /// start or runs out of time is, and goes to the plugin's runs.
    pub fn run(&self, pane: &Pane) -> Result<Option<String>, String> {
        let started = Instant::now();
        let result = self.run_once(pane);
        if let Some(dir) = self.file.parent() {
            crate::plugin_context::record(
                dir,
                &format!("decorations/{}", self.id),
                &result.as_ref().map(|_| ()).map_err(Clone::clone),
                started.elapsed().as_millis() as u64,
                true,
            );
        }
        result
    }
    /// What its command runs with for `pane`: the plugin's variables and the pane's.
    pub fn env_for(&self, pane: &Pane) -> Vec<(String, String)> {
        let mut env = self
            .file
            .parent()
            .map(crate::plugin_context::env)
            .unwrap_or_default();
        // Plugins written for Fibo read `FIBO_*`.
        env.extend(crate::legacy::with_fibo_names(vec![
            ("MOORING_CWD".to_owned(), pane.cwd.clone()),
            (
                "MOORING_AGENT".to_owned(),
                pane.agent.clone().unwrap_or_default(),
            ),
            (
                "MOORING_SESSION".to_owned(),
                pane.session.clone().unwrap_or_default(),
            ),
            (
                "MOORING_LAST_EXIT".to_owned(),
                pane.last
                    .and_then(|l| l.exit)
                    .map(|e| e.to_string())
                    .unwrap_or_default(),
            ),
            (
                "MOORING_LAST_DURATION_MS".to_owned(),
                pane.last.map(|l| l.took_ms.to_string()).unwrap_or_default(),
            ),
        ]));
        env
    }
    /// What runs for `pane`. A line is one shot: whatever it left in the background would
    /// run unseen, so it ends with the command.
    pub fn spec<'a>(&self, pane: &'a Pane) -> crate::plugin_command::Run<'a> {
        use crate::plugin_command::{Leftovers, Run};
        Run {
            cwd: Some(Path::new(&pane.cwd)),
            env: self.env_for(pane),
            ..Run::shell(
                &self.command,
                Duration::from_millis(self.timeout_ms),
                Leftovers::End,
            )
        }
    }
    fn run_once(&self, pane: &Pane) -> Result<Option<String>, String> {
        use crate::plugin_command::Failed;
        let done = crate::plugin_command::run(self.spec(pane)).map_err(|e| match e {
            Failed::Start(why) => t!("启动不了：{why}", why),
            Failed::TimedOut => t!("超过 {ms} ms，已停止", ms = self.timeout_ms),
        })?;
        // A script opting out exits non-zero; one the shell cannot even run has not opted
        // out of anything, and its author needs to hear why.
        if done.not_runnable() {
            return Err(t!("{display}：{e}", display = done.how(), e = done.said()));
        }
        if !done.status.success() {
            return Ok(None);
        }
        let out = done.stdout;
        let limit = match self.place {
            Place::Title => TITLE_CHARS,
            Place::Tip => TIP_CHARS,
        };
        Ok(String::from_utf8_lossy(&out)
            .lines()
            .map(|l| strip_controls(l.trim()))
            .find(|l| !l.is_empty())
            .map(|l| crate::content::short(&l, limit)))
    }
}

/// Text only: escape sequences and other control characters a command printed are dropped.
fn strip_controls(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // CSI: ESC [ … a final byte in @..~. OSC: ESC ] … BEL or ESC \. Anything
            // else: the one character after ESC.
            match chars.next() {
                Some('[') => while chars.next().is_some_and(|c| !('@'..='~').contains(&c)) {},
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if !c.is_control() {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    api: u32,
    decorations: Vec<Decoration>,
}

fn read(file: &Path) -> Result<Vec<Decoration>, String> {
    let bytes = std::fs::read(file).map_err(|e| t!("读不了：{e}", e))?;
    let f: File = serde_json::from_slice(&bytes).map_err(|e| crate::plugin::bad_format(&e))?;
    if f.api != API {
        return Err(t!(
            "装饰格式是 api {got}，这个版本只认 api {API}",
            got = f.api,
            API
        ));
    }
    if f.decorations.is_empty() {
        return Err(t!("decorations 至少写一条").into());
    }
    let mut found = f.decorations;
    for i in 0..found.len() {
        let (before, rest) = found.split_at_mut(i);
        let d = &mut rest[0];
        if !crate::plugin::valid_id(&d.id) {
            return Err(t!(
                "装饰 id \"{id}\" 只能用小写字母、数字和 -，不超过 40 个字",
                id = d.id
            ));
        }
        // Inside its own plugin only: nothing stores a decoration's id (the cache goes by
        // position), so two plugins may share one and installing one cannot break another.
        if before.iter().any(|t| t.id == d.id) {
            return Err(t!("装饰 id \"{id}\" 重名", id = d.id));
        }
        if d.command.trim().is_empty() {
            return Err(t!("装饰 {id} 要写 command", id = d.id));
        }
        if !(2..=3600).contains(&d.refresh_s) {
            return Err(t!("装饰 {id} 的 refresh_s 要在 2 到 3600 之间", id = d.id));
        }
        if !(100..=MAX_TIMEOUT_MS).contains(&d.timeout_ms) {
            return Err(t!(
                "装饰 {id} 的 timeout_ms 要在 100 到 {MAX_TIMEOUT_MS} 之间",
                id = d.id,
                MAX_TIMEOUT_MS
            ));
        }
        d.file = file.to_owned();
    }
    Ok(found)
}

pub fn load(files: Vec<PathBuf>) -> (Vec<Decoration>, Vec<Loaded>) {
    let mut all: Vec<Decoration> = Vec::new();
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
                        .map(|d| d.id.as_str())
                        .collect::<Vec<_>>()
                        .join(t!("、"))
                })
                .map_err(Clone::clone),
        });
        if let Ok(found) = result {
            all.extend(found);
        }
    }
    (all, report)
}

static DECORATIONS: crate::plugin::Slot<(Vec<Decoration>, Vec<Loaded>)> =
    crate::plugin::Slot::new();

/// Loads the installed plugins' decorations once (after `plugin::install`).
pub fn install() {
    DECORATIONS.get_or_init(|| load(crate::plugin::files(crate::plugin::Kind::Decorations)));
}
/// Loads them again after plugins changed (`plugin::reload`).
pub(crate) fn reload() {
    DECORATIONS.set(load(crate::plugin::files(crate::plugin::Kind::Decorations)));
}
pub fn all() -> &'static [Decoration] {
    DECORATIONS.get().map_or(&[], |(d, _)| d.as_slice())
}
pub fn report() -> &'static [Loaded] {
    DECORATIONS.get().map_or(&[], |(_, r)| r.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoration(command: &str, place: Place, timeout_ms: u64) -> Decoration {
        Decoration {
            id: "t".into(),
            command: command.into(),
            place,
            when: When::Always,
            tone: Tone::Muted,
            refresh_s: 10,
            timeout_ms,
            file: PathBuf::new(),
        }
    }
    fn pane(cwd: &Path) -> Pane {
        Pane {
            cwd: cwd.display().to_string(),
            agent: Some("Codex".into()),
            session: Some("s1".into()),
            last: None,
        }
    }

    #[test]
    fn the_first_line_printed_in_the_panes_directory_is_shown_as_text() {
        let dir = std::env::temp_dir().join(format!("kl-deco-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("marker"), "").unwrap();
        let shown =
            |command: &str, place| decoration(command, place, 2_000).run(&pane(&dir)).unwrap();
        assert_eq!(
            shown("\n  ls; echo second", Place::Title).as_deref(),
            Some("marker")
        );
        assert_eq!(
            shown(
                "printf '%s|%s' \"$MOORING_AGENT\" \"$MOORING_SESSION\"",
                Place::Title
            )
            .as_deref(),
            Some("Codex|s1")
        );
        let failed = Pane {
            last: Some(crate::protocol::LastCommand {
                exit: Some(3),
                took_ms: 1200,
            }),
            ..pane(&dir)
        };
        assert_eq!(
            decoration(
                "printf '%s|%s|' \"$MOORING_LAST_EXIT\" \"$MOORING_LAST_DURATION_MS\"",
                Place::Title,
                2_000
            )
            .run(&failed)
            .unwrap()
            .as_deref(),
            Some("3|1200|")
        );
        assert_eq!(
            shown("printf '[%s]' \"$MOORING_LAST_EXIT\"", Place::Title).as_deref(),
            Some("[]"),
            "empty before the first command"
        );
        assert_eq!(
            shown(r"printf '\033[31m红\033[0m\a\033]0;x\a ok'", Place::Title).as_deref(),
            Some("红 ok"),
            "colors, titles and bells are dropped, text kept"
        );
        assert_eq!(shown("echo nope; exit 1", Place::Title), None);
        assert_eq!(shown("true", Place::Title), None);
        let long = "字".repeat(200);
        let title = shown(&format!("echo {long}"), Place::Title).unwrap();
        let tip = shown(&format!("echo {long}"), Place::Tip).unwrap();
        assert!(title.chars().count() <= TITLE_CHARS + 1, "{title}");
        assert!(tip.chars().count() > TITLE_CHARS + 1 && tip.chars().count() <= TIP_CHARS + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_slow_or_noisy_command_is_stopped_at_its_timeout() {
        let dir = std::env::temp_dir();
        let at = Instant::now();
        let e = decoration("sleep 5", Place::Title, 200)
            .run(&pane(&dir))
            .unwrap_err();
        assert!(e.contains("200 ms"), "{e}");
        assert!(at.elapsed() < Duration::from_secs(2));
        // Endless output is read only so far, and the command still ends.
        let at = Instant::now();
        let _ = decoration("yes", Place::Title, 300).run(&pane(&dir));
        assert!(at.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn only_sane_declarations_load() {
        let d = std::env::temp_dir().join(format!("kl-deco-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let write = |name: &str, body: &str| {
            let p = d.join(name);
            std::fs::write(&p, body).unwrap();
            p
        };
        let good = write(
            "a.json",
            r#"{"api":1,"decorations":[{"id":"node","command":"node -v","when":"shell"},{"id":"ctx","command":"kubectl config current-context","place":"tip","tone":"warn","refresh_s":60,"timeout_ms":3000}]}"#,
        );
        let fast = write(
            "b.json",
            r#"{"api":1,"decorations":[{"id":"x","command":"true","refresh_s":1}]}"#,
        );
        let place = write(
            "c.json",
            r#"{"api":1,"decorations":[{"id":"y","command":"true","place":"footer"},{"id":"z","command":"true","tone":"red"}]}"#,
        );
        let dup = write(
            "d.json",
            r#"{"api":1,"decorations":[{"id":"x","command":"true"},{"id":"x","command":"true"}]}"#,
        );
        let (all, report) = load(vec![good, fast, place, dup]);
        assert_eq!(all.len(), 2);
        assert_eq!(
            (all[0].place, all[0].when, all[0].tone, all[0].refresh_s),
            (Place::Title, When::Shell, Tone::Muted, 10)
        );
        assert_eq!(
            (all[1].place, all[1].tone, all[1].timeout_ms),
            (Place::Tip, Tone::Warn, 3000)
        );
        assert_eq!(report[0].result, Ok("node、ctx".into()));
        assert!(report[1].result.as_ref().unwrap_err().contains("refresh_s"));
        assert!(report[2].result.as_ref().unwrap_err().contains("格式不对"));
        assert!(report[3].result.as_ref().unwrap_err().contains("重名"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A command that leaves a background child holding stdout still gives up at its
    /// timeout, and the child ends with it.
    #[test]
    fn what_a_command_leaves_running_ends_with_it() {
        let dir = std::env::temp_dir();
        let start = std::time::Instant::now();
        let shown = decoration("echo hi; sleep 3 &", Place::Title, 2_000).run(&pane(&dir));
        assert!(
            start.elapsed() < Duration::from_millis(1500),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(shown, Ok(Some("hi".into())));
        let start = std::time::Instant::now();
        let _ = decoration("sleep 3 & sleep 3", Place::Title, 200).run(&pane(&dir));
        assert!(
            start.elapsed() < Duration::from_millis(1500),
            "{:?}",
            start.elapsed()
        );
    }

    /// A script opting out stays quiet. A command that does not exist is not opting out:
    /// it is reported, with what the shell said, and goes to the plugin's runs.
    #[test]
    fn a_command_that_cannot_run_is_a_failure_not_an_opt_out() {
        let root = std::env::temp_dir().join(format!("kl-deco-127-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("plugins/d");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"api":1,"id":"d","name":"d","provides":["decorations"]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("decorations.json"), "{}").unwrap();
        let mut d = decoration("[ -d / ] && no-such-tool-xyz -v", Place::Title, 2_000);
        d.file = dir.join("decorations.json");
        let cwd = std::env::temp_dir();
        let err = d.run(&pane(&cwd)).unwrap_err();
        assert!(
            err.contains("退出码 127") && err.contains("no-such-tool-xyz"),
            "{err}"
        );
        let runs = crate::plugin_context::runs(&root.join("plugins"), "d");
        assert_eq!(runs.len(), 1);
        assert!(!runs[0].ok && runs[0].detail.contains("127"), "{runs:?}");
        d.command = "exit 1".into();
        assert_eq!(d.run(&pane(&cwd)), Ok(None));
        let _ = std::fs::remove_dir_all(&root);
    }
}
