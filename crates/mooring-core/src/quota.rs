//! Account usage limits. Built in: what a CLI records on this machine anyway, Codex in
//! its conversation logs and Grok in its own log when it starts. Everything else comes
//! from plugins (`quota.json`): a file a plugin keeps up to date (the `claude-quota`
//! plugin saves what Claude Code hands its status line) or a command it runs. Mooring
//! itself never asks a provider: a reading is only as fresh as its source, so every use
//! keeps its time. A plugin's command may go online, and says so in its manifest.
use crate::agent::Loaded;
use crate::model::{Agent, Workspace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Readings older than this are left out: that CLI has not been used for a week.
pub const SHOWN_FOR: u64 = 7 * 86_400;
/// From here a window shows as nearly used up.
pub const NEAR: u16 = 80;
/// From here a window shows as used up.
pub const OVER: u16 = 95;
/// A long window this close to renewing, with this much of it left, is about to go to waste.
pub const SPARE_WITHIN: u64 = 86_400;
pub const SPARE_FROM: u16 = 30;
/// Windows shorter than this renew too often for what is left of them to matter.
const SPARE_WINDOW_MINUTES: u32 = 10_080;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quota {
    /// When the CLI recorded it, Unix seconds.
    pub at: u64,
    /// The plan the CLI reported (plus, pro, max…); empty when it gave none.
    #[serde(default)]
    pub plan: String,
    /// Shortest window first.
    pub windows: Vec<Window>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub minutes: u32,
    /// Percent used as the provider reported it, rounded; above 100 once exceeded.
    pub used: u16,
    /// Unix seconds; 0 when the CLI gave no time.
    pub resets_at: u64,
    /// What it counts when not everything, such as one model's week. Empty for the account's own.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}
impl Quota {
    /// A reading older than an hour is historical, even if its window has not reset.
    pub fn fresh_until(&self) -> u64 {
        self.at.saturating_add(3600)
    }

    pub fn stale(&self, now: u64) -> bool {
        now > self.fresh_until()
    }

    /// Most used first; ties keep the source's shortest-window-first order.
    pub fn by_usage(&self) -> Vec<&Window> {
        let mut windows: Vec<_> = self.windows.iter().collect();
        windows.sort_by_key(|w| std::cmp::Reverse(w.used));
        windows
    }
    pub fn tightest(&self, now: u64) -> Option<&Window> {
        self.by_usage().into_iter().find(|w| !w.reset(now))
    }
}

impl Window {
    /// Shared presentation severity, also sent to remote clients.
    pub fn level(&self) -> &'static str {
        if self.used >= OVER {
            "high"
        } else if self.used >= NEAR {
            "mid"
        } else {
            "low"
        }
    }

    /// The windows CLIs report today get their usual names; any other its length. One with a
    /// name of its own says it, or two weeks read the same.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            self.length_label()
        } else {
            format!("{} {}", self.name, self.length_label())
        }
    }
    /// Whether the status bar has room for it: the account's own windows, and a narrower one
    /// only once it may stop the user. The popover lists them all.
    pub fn on_bar(&self) -> bool {
        self.name.is_empty() || self.used >= NEAR
    }
    fn length_label(&self) -> String {
        match self.minutes {
            300 => "5h".into(),
            10_080 => crate::t!("周").into(),
            43_200 => crate::t!("月").into(),
            m if m % 1440 == 0 => crate::t!("{n}天", n = m / 1440),
            m if m % 60 == 0 => format!("{}h", m / 60),
            m => crate::t!("{m}分", m),
        }
    }
    /// The window rolled over after the reading, so its percentage no longer applies.
    pub fn reset(&self, now: u64) -> bool {
        self.resets_at != 0 && now >= self.resets_at
    }
    /// How much of a week or longer will go unused when it renews within a day: the last
    /// reading's share, so at most this much if it was used since. None when not worth saying.
    pub fn spare(&self, now: u64) -> Option<u16> {
        // One model's week left unused is no waste: the account's own is what was paid for.
        (self.name.is_empty()
            && self.minutes >= SPARE_WINDOW_MINUTES
            && self.resets_at > now
            && self.resets_at - now <= SPARE_WITHIN
            && 100u16.saturating_sub(self.used) >= SPARE_FROM)
            .then(|| 100 - self.used)
    }
}

/// The newest reading of each CLI used in the last week, in the app's agent order: from
/// its sessions and from plugins (`plugins`, what `from_plugins` returned). A named window only
/// another recent reading knows stays beside it (`quota_merge`).
pub fn latest(w: &Workspace, plugins: &[(Agent, Quota)], now: u64) -> Vec<(Agent, Quota)> {
    let mut readings: HashMap<Agent, Vec<&Quota>> = HashMap::new();
    let sessions = w.agent_sessions.iter().filter_map(|s| {
        w.content
            .get(&s.id)
            .and_then(|c| c.quota.as_ref())
            .map(|q| (s.runtime, q))
    });
    for (agent, q) in sessions.chain(plugins.iter().map(|(a, q)| (*a, q))) {
        if q.windows.is_empty() || now.saturating_sub(q.at) > SHOWN_FOR {
            continue;
        }
        readings.entry(agent).or_default().push(q);
    }
    let order = Agent::all();
    let mut out: Vec<_> = readings
        .into_iter()
        .map(|(a, found)| (a, crate::quota_merge::newest(&found, now)))
        .collect();
    out.sort_by_key(|(a, _)| {
        (
            order.iter().position(|o| o == a).unwrap_or(order.len()),
            a.key(),
        )
    });
    out
}

/// The `quota.json` format's major version.
pub const API: u32 = 1;

/// One Agent's limits as a plugin provides them: a JSON document, from a file or from a
/// command's output, and where in it each window's numbers are (JSON pointers).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The Agent's executable (`claude`) or name (`Claude`).
    pub agent: String,
    /// A JSON file: relative to the plugin's data directory (`MOORING_PLUGIN_DATA`), or
    /// absolute, or under `~/`. Read again whenever it changes.
    #[serde(default)]
    pub file: Option<String>,
    /// Or a command printing the JSON, run with `sh -c` in the plugin's directory every
    /// `refresh_s` while the app runs.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default = "default_refresh")]
    pub refresh_s: u64,
    /// How long a command may take (plugin level 8). A CLI asked for its limits on a busy
    /// machine can take far longer than the default.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// When it was recorded, Unix seconds; without it, when the file last changed or the
    /// command ran.
    #[serde(default)]
    pub at: Option<String>,
    /// The plan's name, if the document has one.
    #[serde(default)]
    pub plan: Option<String>,
    pub windows: Vec<WindowField>,
    /// The plugin's directory and data directory.
    #[serde(skip)]
    pub dir: PathBuf,
    #[serde(skip)]
    pub data: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowField {
    pub minutes: u32,
    /// Percent used, a number.
    pub used: String,
    /// When it resets, Unix seconds.
    #[serde(default)]
    pub resets_at: Option<String>,
    /// Its own name (plugin level 8), for a window counting less than the whole account.
    #[serde(default)]
    pub name: Option<String>,
}
fn default_refresh() -> u64 {
    300
}
/// How long a command may take.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);

impl Source {
    /// The Agent it is for, once the registry knows it.
    pub fn agent(&self) -> Option<Agent> {
        Agent::from_executable(&self.agent).or_else(|| {
            Agent::all()
                .into_iter()
                .find(|a| a.key().eq_ignore_ascii_case(&self.agent))
        })
    }
    fn path(&self) -> Option<PathBuf> {
        let f = self.file.as_deref()?;
        Some(if let Some(rest) = f.strip_prefix("~/") {
            mooring_platform::dirs::home()?.join(rest)
        } else if Path::new(f).is_absolute() {
            PathBuf::from(f)
        } else {
            self.data.join(f)
        })
    }
    /// The reading in `doc`; `at` when the document does not say when. None without a
    /// window: an account with no limits there (an API key) shows nothing.
    pub fn reading(&self, doc: &Value, at: u64) -> Option<Quota> {
        let num = |ptr: &str| doc.pointer(ptr).and_then(Value::as_f64);
        let windows: Vec<Window> = self
            .windows
            .iter()
            .filter_map(|w| {
                Some(Window {
                    minutes: w.minutes,
                    used: num(&w.used)?.round().clamp(0., 999.) as u16,
                    resets_at: w
                        .resets_at
                        .as_deref()
                        .and_then(num)
                        .map_or(0, |t| t.max(0.) as u64),
                    name: w
                        .name
                        .as_deref()
                        .and_then(|p| doc.pointer(p))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .collect();
        (!windows.is_empty()).then(|| Quota {
            at: self
                .at
                .as_deref()
                .and_then(num)
                .map_or(at, |t| t.max(0.) as u64),
            plan: self
                .plan
                .as_deref()
                .and_then(|p| doc.pointer(p))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            windows,
        })
    }
    /// What runs for a command source. It was asked for one document; nothing it started
    /// has to outlive that.
    pub fn spec(&self) -> crate::plugin_command::Run<'_> {
        use crate::plugin_command::{Leftovers, Run};
        Run {
            cwd: Some(&self.dir),
            env: crate::plugin_context::env(&self.dir),
            keep: 1024 * 1024,
            ..Run::shell(
                self.command.as_deref().unwrap_or_default(),
                self.timeout_ms
                    .map_or(COMMAND_TIMEOUT, Duration::from_millis),
                Leftovers::End,
            )
        }
    }
    fn run(&self) -> Result<Value, String> {
        use crate::plugin_command::Failed;
        let done = crate::plugin_command::run(self.spec()).map_err(|e| match e {
            Failed::Start(why) => why,
            Failed::TimedOut => t!("超时#quota").to_owned(),
        })?;
        if !done.status.success() {
            return Err(t!(
                "{how}：{said}#quota",
                how = done.how(),
                said = done.said()
            ));
        }
        serde_json::from_slice(&done.stdout).map_err(|e| e.to_string())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    api: u32,
    sources: Vec<Source>,
}

fn read(file: &Path) -> Result<Vec<Source>, String> {
    let bytes = std::fs::read(file).map_err(|e| t!("读不了：{e}#runtime", e))?;
    let f: File = serde_json::from_slice(&bytes).map_err(|e| crate::plugin::bad_format(&e))?;
    if f.api != API {
        return Err(t!(
            "额度格式是 api {api}，这个版本只认 api {max}",
            api = f.api,
            max = API
        ));
    }
    if f.sources.is_empty() {
        return Err(t!("sources 至少写一条").into());
    }
    let dir = file.parent().unwrap_or(Path::new("/")).to_owned();
    let id = crate::plugin::check(&dir).map(|p| p.id)?;
    let data = crate::setup::data_dir(dir.parent().unwrap_or(&dir), &id);
    let mut sources = f.sources;
    for s in &mut sources {
        let who = &s.agent;
        if who.trim().is_empty() {
            return Err(t!("agent 不能为空").into());
        }
        if s.file.is_some() == s.command.is_some() {
            return Err(t!("{who}：file 和 command 写一个", who));
        }
        if !(60..=86_400).contains(&s.refresh_s) {
            return Err(t!("{who}：refresh_s 要在 60 到 86400 之间", who));
        }
        if s.timeout_ms.is_some_and(|t| !(100..=300_000).contains(&t)) {
            return Err(t!("{who}：timeout_ms 要在 100 到 300000 之间", who));
        }
        if s.windows.is_empty() {
            return Err(t!("{who}：windows 至少写一个", who));
        }
        let pointers = s
            .windows
            .iter()
            .flat_map(|w| [Some(&w.used), w.resets_at.as_ref(), w.name.as_ref()]);
        if let Some(bad) = pointers
            .chain([s.at.as_ref(), s.plan.as_ref()])
            .flatten()
            .find(|p| !p.starts_with('/'))
        {
            return Err(t!("{who}：\"{bad}\" 要写成 JSON 指针，以 / 开头", who, bad));
        }
        if s.windows.iter().any(|w| w.minutes == 0) {
            return Err(t!("{who}：窗口的 minutes 要大于 0", who));
        }
        s.dir = dir.clone();
        s.data = data.clone();
    }
    Ok(sources)
}

pub fn load(files: Vec<PathBuf>) -> (Vec<Source>, Vec<Loaded>) {
    let mut all = Vec::new();
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
                        .map(|s| s.agent.as_str())
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

static SOURCES: crate::plugin::Slot<(Vec<Source>, Vec<Loaded>)> = crate::plugin::Slot::new();
/// Bumped on every reload, so the command loops of the sources replaced stop.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The last reading of each command source, by generation and index.
static RAN: Mutex<Option<(u64, HashMap<usize, Quota>)>> = Mutex::new(None);
/// File sources' last parsed document, by path and modification time. The document and not
/// the reading: two sources may read one file through different pointers, and a reloaded
/// source may change its pointers while the file stays as it was.
type FileReadings = HashMap<PathBuf, (SystemTime, Option<Value>)>;
static READ: Mutex<Option<FileReadings>> = Mutex::new(None);

/// Loads the installed plugins' sources once (after `plugin::install`).
pub fn install() {
    SOURCES.get_or_init(|| load(crate::plugin::files(crate::plugin::Kind::Quota)));
}
/// Loads them again after plugins changed (`plugin::reload`).
pub(crate) fn reload() {
    if SOURCES.get().is_none() {
        return;
    }
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    SOURCES.set(load(crate::plugin::files(crate::plugin::Kind::Quota)));
}
pub fn report() -> &'static [Loaded] {
    SOURCES.get().map_or(&[], |(_, r)| r.as_slice())
}

/// Every plugin's current reading. Files are read again when they changed; a command
/// source starts its loop the first time and shows its last good reading.
pub fn from_plugins() -> Vec<(Agent, Quota)> {
    let Some((sources, _)) = SOURCES.get() else {
        return vec![];
    };
    let generation = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    let mut out = Vec::new();
    let mut ran = RAN.lock().unwrap_or_else(|e| e.into_inner());
    if ran.as_ref().is_none_or(|(g, _)| *g != generation) {
        *ran = Some((generation, HashMap::new()));
        for (i, s) in sources.iter().enumerate() {
            if s.command.is_some() {
                start(i, s.clone(), generation);
            }
        }
    }
    let ran = ran.as_ref().map(|(_, r)| r.clone()).unwrap_or_default();
    let mut read = READ.lock().unwrap_or_else(|e| e.into_inner());
    let read = read.get_or_insert_with(HashMap::new);
    for (i, s) in sources.iter().enumerate() {
        let Some(agent) = s.agent() else { continue };
        let q = match s.path() {
            Some(path) => {
                let Ok(changed) = std::fs::metadata(&path).and_then(|m| m.modified()) else {
                    continue;
                };
                let at = changed
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                if read.get(&path).is_none_or(|(t, _)| *t != changed) {
                    let doc = std::fs::read(&path)
                        .ok()
                        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
                    read.insert(path.clone(), (changed, doc));
                }
                read.get(&path)
                    .and_then(|(_, doc)| doc.as_ref())
                    .and_then(|doc| s.reading(doc, at))
            }
            None => ran.get(&i).cloned(),
        };
        if let Some(q) = q {
            out.push((agent, q));
        }
    }
    out
}

fn start(i: usize, source: Source, generation: u64) {
    std::thread::spawn(move || {
        let current = || GENERATION.load(std::sync::atomic::Ordering::SeqCst) == generation;
        while current() {
            let started = Instant::now();
            let ran = source.run();
            crate::plugin_context::record(
                &source.dir,
                &format!("quota/{}", source.agent),
                &ran.as_ref().map(|_| ()).map_err(Clone::clone),
                started.elapsed().as_millis() as u64,
                true,
            );
            if let Ok(doc) = ran {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                // A failed run never gets here: not knowing is not having none.
                if let Some((g, readings)) = RAN.lock().unwrap_or_else(|e| e.into_inner()).as_mut()
                    && *g == generation
                {
                    crate::quota_merge::settle(readings, i, source.reading(&doc, now));
                }
            }
            let until = Instant::now() + Duration::from_secs(source.refresh_s);
            while current() && Instant::now() < until {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_long_window_renewing_soon_with_much_left_counts_as_spare() {
        let now = 1_000_000;
        let week = |used, left| Window {
            minutes: 10_080,
            used,
            resets_at: now + left,
            name: String::new(),
        };
        assert_eq!(week(40, 3600).spare(now), Some(60));
        assert_eq!(week(70, SPARE_WITHIN).spare(now), Some(30));
        // Too much used, too far off, already renewed, or no time given.
        assert_eq!(week(71, 3600).spare(now), None);
        assert_eq!(week(40, SPARE_WITHIN + 1).spare(now), None);
        assert_eq!(week(40, 0).spare(now), None);
        assert_eq!(
            Window {
                resets_at: 0,
                ..week(40, 1)
            }
            .spare(now),
            None
        );
        // Over its limit: nothing left, and no underflow.
        assert_eq!(week(130, 3600).spare(now), None);
        // The five-hour window renews all day long.
        assert_eq!(
            Window {
                minutes: 300,
                ..week(10, 3600)
            }
            .spare(now),
            None
        );
        let month = Window {
            minutes: 43_200,
            ..week(10, 3600)
        };
        assert_eq!(month.spare(now), Some(90));
    }
    use crate::content::Content;
    use crate::model::{AgentSession, Outcome};

    fn session(id: &str, runtime: Agent) -> AgentSession {
        AgentSession {
            id: id.into(),
            runtime,
            cli_session_id: id.into(),
            terminal: None,
            current: false,
            task: None,
            outcome: Outcome::Open,
            updated: 0,
            cwd: "/tmp".into(),
            transcript: String::new(),
            binding: None,
            name: String::new(),
        }
    }
    fn reading(at: u64, used: u16) -> Content {
        Content {
            quota: Some(Quota {
                at,
                plan: String::new(),
                windows: vec![Window {
                    minutes: 300,
                    used,
                    resets_at: at + 3600,
                    name: String::new(),
                }],
            }),
            ..Default::default()
        }
    }

    #[test]
    fn newest_reading_per_agent_in_agent_order() {
        let now = 10 * 86_400;
        let mut w = Workspace {
            agent_sessions: vec![
                session("a", Agent::Codex),
                session("b", Agent::Codex),
                session("c", Agent::Claude),
                session("old", Agent::Grok),
                session("none", Agent::Cursor),
            ],
            ..Default::default()
        };
        w.content.insert("a".into(), reading(now - 600, 40));
        w.content.insert("b".into(), reading(now - 60, 42));
        w.content.insert("c".into(), reading(now - 30, 7));
        w.content
            .insert("old".into(), reading(now - SHOWN_FOR - 1, 99));
        w.content.insert("none".into(), Content::default());
        let got: Vec<_> = latest(&w, &[], now)
            .into_iter()
            .map(|(a, q)| (a.key(), q.windows[0].used))
            .collect();
        assert_eq!(got, [("Codex", 42), ("Claude", 7)]);
    }

    #[test]
    fn labels_and_rollover() {
        let w = |minutes| Window {
            minutes,
            used: 0,
            resets_at: 100,
            name: String::new(),
        };
        assert_eq!(w(300).label(), "5h");
        assert_eq!(w(10_080).label(), "周");
        assert_eq!(w(43_200).label(), "月");
        assert_eq!(w(2880).label(), "2天");
        assert_eq!(w(120).label(), "2h");
        assert!(w(300).reset(100) && !w(300).reset(99));
        assert!(!Window::default().reset(u64::MAX));
    }

    #[test]
    fn a_plugin_reading_counts_when_newer_than_the_sessions() {
        let now = 10 * 86_400;
        let mut w = Workspace {
            agent_sessions: vec![session("c", Agent::Claude)],
            ..Default::default()
        };
        w.content.insert("c".into(), reading(now - 600, 7));
        let plugin = |at, used| reading(at, used).quota.unwrap();
        let newer = [(Agent::Claude, plugin(now - 60, 30))];
        let older = [(Agent::Claude, plugin(now - 6000, 90))];
        let used = |extra: &[(Agent, Quota)]| latest(&w, extra, now)[0].1.windows[0].used;
        assert_eq!(used(&newer), 30);
        assert_eq!(used(&older), 7);
        let only = [(Agent::Grok, plugin(now - 60, 5))];
        assert_eq!(latest(&Workspace::default(), &only, now).len(), 1);
    }

    #[test]
    fn the_claude_plugin_reads_what_its_status_line_kept() {
        let root = std::env::temp_dir().join(format!("kl-quota-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("plugins/claude-quota");
        std::fs::create_dir_all(&dir).unwrap();
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/claude-quota");
        for f in ["plugin.json", "quota.json", "setup.json"] {
            std::fs::copy(shipped.join(f), dir.join(f)).unwrap();
        }
        let (sources, report) = load(vec![dir.join("quota.json")]);
        assert!(report[0].result.is_ok(), "{report:?}");
        let s = &sources[0];
        assert_eq!(
            s.path().unwrap(),
            root.join("plugin-data/claude-quota/statusline.json")
        );
        let doc = serde_json::json!({
            "session_id": "abc",
            "rate_limits": {
                "five_hour": {"used_percentage": 23.6, "resets_at": 1_798_000_000},
                "seven_day": {"used_percentage": 41, "resets_at": 1_798_400_000},
                "seven_day_opus": null
            }
        });
        let q = s.reading(&doc, 1_793_605_648).unwrap();
        assert_eq!(q.at, 1_793_605_648);
        assert_eq!(
            q.windows,
            [
                Window {
                    minutes: 300,
                    used: 24,
                    resets_at: 1_798_000_000,
                    name: String::new(),
                },
                Window {
                    minutes: 10_080,
                    used: 41,
                    resets_at: 1_798_400_000,
                    name: String::new(),
                },
            ]
        );
        // API-key sign-ins: no limits, nothing to show.
        assert_eq!(
            s.reading(&serde_json::json!({"rate_limits": null}), 1),
            None
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_source_says_where_and_how() {
        let root = std::env::temp_dir().join(format!("kl-quota-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("plugins/x");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"api":1,"id":"x","name":"X","provides":["quota"]}"#,
        )
        .unwrap();
        let check = |body: &str| {
            std::fs::write(dir.join("quota.json"), body).unwrap();
            load(vec![dir.join("quota.json")]).1[0].result.clone()
        };
        let w = r#""windows":[{"minutes":300,"used":"/u"}]"#;
        assert!(
            check(&format!(r#"{{"api":1,"sources":[{{"agent":"a",{w}}}]}}"#))
                .unwrap_err()
                .contains("写一个")
        );
        assert!(check(r#"{"api":1,"sources":[{"agent":"a","file":"f","windows":[{"minutes":300,"used":"u"}]}]}"#)
            .unwrap_err()
            .contains("JSON 指针"));
        assert_eq!(
            check(&format!(
                r#"{{"api":1,"sources":[{{"agent":"a","command":"echo {{}}",{w}}}]}}"#
            )),
            Ok("a".into())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One file, two sources, different pointers: each reads its own number, and a pointer
    /// changed in the declaration takes effect though the file did not change.
    #[test]
    fn sources_sharing_a_file_each_read_their_own_pointer() {
        let root = std::env::temp_dir().join(format!("kl-quota-shared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("plugins/q");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"api":1,"id":"q","name":"q","provides":["quota"]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("quota.json"),
            r#"{"api":1,"sources":[
              {"agent":"claude","file":"s.json","windows":[{"minutes":300,"used":"/claude"}]},
              {"agent":"codex","file":"s.json","windows":[{"minutes":300,"used":"/codex"}]}]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("plugin-data/q")).unwrap();
        std::fs::write(
            root.join("plugin-data/q/s.json"),
            r#"{"claude":11,"codex":77}"#,
        )
        .unwrap();
        crate::plugin::install(&root);
        install();
        let used = || -> Vec<(String, u16)> {
            from_plugins()
                .into_iter()
                .map(|(a, q)| (a.key().to_owned(), q.windows[0].used))
                .collect()
        };
        assert_eq!(
            used(),
            [("Claude".to_owned(), 11), ("Codex".to_owned(), 77)]
        );
        std::fs::write(
            dir.join("quota.json"),
            r#"{"api":1,"sources":[{"agent":"claude","file":"s.json","windows":[{"minutes":300,"used":"/codex"}]}]}"#,
        )
        .unwrap();
        crate::plugin::reload();
        assert_eq!(used(), [("Claude".to_owned(), 77)]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
