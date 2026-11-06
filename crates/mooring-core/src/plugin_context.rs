//! What every plugin command is handed, and what is kept of how it went.
//!
//! Every command a plugin declares (hooks, decorations, quota commands, setup and
//! teardown) runs with the same variables beside its kind's own:
//!
//! - `MOORING_PLUGIN_ID`, `MOORING_PLUGIN_DIR`: which plugin, and its directory, so a
//!   script finds the files it ships (`files` in `plugin.json`).
//! - `MOORING_PLUGIN_DATA`: its own directory under `<config>/plugin-data/<id>`, kept
//!   across updates and removed on uninstall.
//! - `MOORING_BIN`, `MOORING_RUNTIME`: the `mooring` command and the host it talks to, so
//!   `"$MOORING_BIN" list` works from a plugin as it does in a Mooring terminal.
//! - `MOORING_OPTION_<KEY>`: each of its settings (`options` in `plugin.json`), as the user
//!   set them in Settings › 插件 or the default. Values are only ever variables: they are
//!   never put into the command text, so a value cannot become part of a command.
//!
//! Runs are kept in `<data>/runs.jsonl`, the last `KEPT`: setup, teardown and hooks every
//! time, a decoration or quota command only when its outcome changes, so a command that
//! runs every ten seconds writes nothing while it keeps working.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Runs kept per plugin.
pub const KEPT: usize = 50;
const MAX_OPTIONS: usize = 20;

/// One setting a plugin offers.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Opt {
    /// Lowercase letters, digits and `_`; the variable is `MOORING_OPTION_<KEY>` upper-cased.
    pub key: String,
    pub title: String,
    #[serde(rename = "type")]
    pub kind: OptKind,
    pub default: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The values a `choice` takes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// Kept in the keychain, never in a file; a `string` with an empty default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OptKind {
    String,
    Number,
    Boolean,
    Choice,
}
impl Opt {
    /// Whether `v` is a value of this setting.
    pub fn accepts(&self, v: &Value) -> bool {
        match self.kind {
            OptKind::String => v.is_string(),
            OptKind::Number => v.is_number(),
            OptKind::Boolean => v.is_boolean(),
            OptKind::Choice => v
                .as_str()
                .is_some_and(|s| self.choices.iter().any(|c| c == s)),
        }
    }
    /// The value as the user would type it; a variable's text.
    pub fn text(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Bool(b) => if *b { "1" } else { "0" }.into(),
            other => other.to_string(),
        }
    }
    /// Reads typed text as this setting's value.
    pub fn parse(&self, text: &str) -> Result<Value, String> {
        let text = text.trim();
        let v = match self.kind {
            OptKind::String | OptKind::Choice => Value::String(text.to_owned()),
            OptKind::Number => text
                .parse::<f64>()
                .ok()
                .and_then(|n| serde_json::Number::from_f64(n).map(Value::Number))
                .ok_or_else(|| format!("{} 要填数字", self.title))?,
            OptKind::Boolean => match text {
                "1" | "true" | "on" | "yes" | "开" => Value::Bool(true),
                "0" | "false" | "off" | "no" | "关" => Value::Bool(false),
                _ => return Err(format!("{} 要填 true 或 false", self.title)),
            },
        };
        if self.accepts(&v) {
            Ok(v)
        } else {
            Err(format!(
                "{} 只能是：{}",
                self.title,
                self.choices.join("、")
            ))
        }
    }
}

/// Checks a manifest's `options`.
pub fn check_options(options: &[Opt]) -> Result<(), String> {
    if options.len() > MAX_OPTIONS {
        return Err(format!("options 最多 {MAX_OPTIONS} 个"));
    }
    for (i, o) in options.iter().enumerate() {
        let key_ok = !o.key.is_empty()
            && o.key.len() <= 40
            && o.key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !key_ok {
            return Err(format!(
                "设置项 \"{}\" 的 key 只能用小写字母、数字和 _",
                o.key
            ));
        }
        if options[..i].iter().any(|p| p.key == o.key) {
            return Err(format!("设置项 {} 重名", o.key));
        }
        if o.title.trim().is_empty() {
            return Err(format!("设置项 {} 要写 title", o.key));
        }
        if o.kind == OptKind::Choice && o.choices.is_empty() {
            return Err(format!("设置项 {} 是 choice，要写 choices", o.key));
        }
        if !o.accepts(&o.default) {
            return Err(format!("设置项 {} 的 default 和 type 对不上", o.key));
        }
        if o.secret && (o.kind != OptKind::String || o.default != Value::String(String::new())) {
            return Err(format!(
                "设置项 {} 是 secret，type 要是 string，default 要是空的",
                o.key
            ));
        }
    }
    Ok(())
}

/// The plugin's plugins directory and id, from its directory.
fn owner(dir: &Path) -> Option<(PathBuf, crate::plugin::Plugin)> {
    let plugins = dir.parent()?.to_owned();
    let p = crate::plugin::check(dir).ok()?;
    Some((plugins, p))
}

fn options_file(plugins: &Path, id: &str) -> PathBuf {
    crate::setup::data_dir(plugins, id).join("options.json")
}

/// What `options.json` holds for a secret that has a value: that it has one, and not what.
pub const SECRET_SET: &str = "已设置";

/// Each setting's current value: what the user set, else the default. A stored value
/// that no longer fits (the plugin changed the setting's type) gives way to the default.
/// For a secret this says only `SECRET_SET` or empty, and asks no keychain: settings draw this.
pub fn values(plugins: &Path, plugin: &crate::plugin::Plugin) -> Vec<(Opt, Value)> {
    let stored: HashMap<String, Value> = std::fs::read(options_file(plugins, &plugin.id))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    plugin
        .options
        .iter()
        .map(|o| {
            let v = stored
                .get(&o.key)
                .filter(|v| o.accepts(v))
                .cloned()
                .unwrap_or_else(|| o.default.clone());
            (o.clone(), v)
        })
        .collect()
}

/// Saves one setting; the next run of any of the plugin's commands sees it.
pub fn set(plugins: &Path, id: &str, key: &str, value: Value) -> Result<(), String> {
    let dir = crate::plugin::discover(plugins)
        .into_iter()
        .find_map(|f| f.result.ok().filter(|p| p.id == id))
        .ok_or_else(|| format!("没有装叫 {id} 的插件"))?;
    let opt = dir
        .options
        .iter()
        .find(|o| o.key == key)
        .ok_or_else(|| format!("{id} 没有叫 {key} 的设置项"))?;
    if !opt.accepts(&value) {
        return Err(format!("{} 的值不对", opt.title));
    }
    let file = options_file(plugins, id);
    let mut stored: serde_json::Map<String, Value> = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if opt.secret {
        let text = value.as_str().unwrap_or_default();
        if text.is_empty() {
            crate::secret::delete(id, key);
            stored.remove(key);
        } else {
            crate::secret::set(id, key, text)?;
            stored.insert(key.to_owned(), SECRET_SET.into());
        }
    } else if value == opt.default {
        stored.remove(key);
    } else {
        stored.insert(key.to_owned(), value);
    }
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&stored).unwrap_or_default())
        .and_then(|()| std::fs::rename(&tmp, &file))
        .map_err(|e| format!("存不了设置：{e}"))
}

/// The `mooring` command next to this program: the app bundle ships it beside the app,
/// and the host is that command itself.
pub fn bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if exe.file_name().is_some_and(|n| n == "mooring") {
        return Some(exe);
    }
    let sibling = exe.parent()?.join("mooring");
    sibling.is_file().then_some(sibling)
}

/// The runtime this process actually opened, when it was told one (`--runtime`).
static RUNTIME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
/// Says which runtime this process opened, so its plugins' `mooring` reaches that host.
pub fn set_runtime(dir: &Path) {
    let _ = RUNTIME.set(dir.to_owned());
}

/// The host's runtime directory as this process knows it.
pub fn runtime() -> Option<PathBuf> {
    RUNTIME
        .get()
        .cloned()
        .or_else(|| std::env::var_os(crate::protocol::RUNTIME_ENV).map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| crate::legacy::runtime_dir(Path::new(&h))))
}

/// The variables every command of the plugin in `dir` runs with.
pub fn env(dir: &Path) -> Vec<(String, String)> {
    let mut out = vec![("MOORING_PLUGIN_DIR".to_owned(), dir.display().to_string())];
    if let Some((plugins, p)) = owner(dir) {
        let data = crate::setup::data_dir(&plugins, &p.id);
        let _ = create_private_dir(&data);
        out.push(("MOORING_PLUGIN_ID".into(), p.id.clone()));
        out.push(("MOORING_PLUGIN_DATA".into(), data.display().to_string()));
        for (o, v) in values(&plugins, &p) {
            let text = if o.secret && v.as_str() == Some(SECRET_SET) {
                crate::secret::get(&p.id, &o.key).unwrap_or_default()
            } else {
                Opt::text(&v)
            };
            out.push((
                format!("MOORING_OPTION_{}", o.key.to_ascii_uppercase()),
                text,
            ));
        }
    }
    if let Some(bin) = bin() {
        out.push(("MOORING_BIN".into(), bin.display().to_string()));
    }
    if let Some(runtime) = runtime() {
        out.push((
            crate::protocol::RUNTIME_ENV.into(),
            runtime.display().to_string(),
        ));
    }
    out
}

#[cfg(unix)]
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// Not private by itself: it takes the rights of the directory it is in, which under the user
/// profile means this account, SYSTEM and administrators. Elsewhere it may mean more people.
#[cfg(windows)]
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// One run of a plugin command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// Unix seconds.
    pub at: u64,
    /// The kind and the declaration: `hooks/notify-macos`, `setup`, `quota/claude`.
    pub what: String,
    pub ok: bool,
    /// Why it failed, or empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub ms: u64,
}

/// The last outcome of each command logged only on change, by data dir and `what`.
static LAST: Mutex<Option<HashMap<(PathBuf, String), bool>>> = Mutex::new(None);

/// Keeps a run of the plugin in `dir`. With `on_change`, only when its outcome differs
/// from this process's last one for the same `what` (the first failure, the recovery).
pub fn record(dir: &Path, what: &str, result: &Result<(), String>, ms: u64, on_change: bool) {
    let Some((plugins, p)) = owner(dir) else {
        return;
    };
    let data = crate::setup::data_dir(&plugins, &p.id);
    let result = &redacted(&plugins, &p, result);
    if on_change {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        let last = last.get_or_insert_with(HashMap::new);
        let key = (data.clone(), what.to_owned());
        let before = last.insert(key, result.is_ok());
        // A command that has always worked is not news; neither is one still failing.
        if before == Some(result.is_ok()) || (before.is_none() && result.is_ok()) {
            return;
        }
    }
    let run = Run {
        at: crate::content::now(),
        what: what.to_owned(),
        ok: result.is_ok(),
        detail: result
            .as_ref()
            .err()
            .map(|e| crate::content::short(e, 300))
            .unwrap_or_default(),
        ms,
    };
    let _ = append(&data, &run);
}

/// Adds a run, keeping the last `KEPT`. The host's hooks, several at once, and the app's
/// decorations write here: a lock around read-trim-replace keeps every run.
/// A command's output is kept and shown: a secret it echoed must not be kept with it.
fn redacted(
    plugins: &Path,
    p: &crate::plugin::Plugin,
    result: &Result<(), String>,
) -> Result<(), String> {
    let Err(why) = result else {
        return Ok(());
    };
    let mut why = why.clone();
    for (o, v) in values(plugins, p) {
        if o.secret
            && v.as_str() == Some(SECRET_SET)
            && let Some(secret) = crate::secret::get(&p.id, &o.key).filter(|s| !s.is_empty())
        {
            why = why.replace(&secret, "••••");
        }
    }
    Err(why)
}

fn append(data: &Path, run: &Run) -> std::io::Result<()> {
    create_private_dir(data)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("runs.lock"))?;
    // Released when it closes.
    lock.lock()?;
    let file = data.join("runs.jsonl");
    let mut lines: Vec<String> = std::fs::read_to_string(&file)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    lines.push(serde_json::to_string(run).map_err(std::io::Error::other)?);
    let from = lines.len().saturating_sub(KEPT);
    let tmp = file.with_extension("jsonl.tmp");
    std::fs::write(&tmp, lines[from..].join("\n") + "\n")?;
    std::fs::rename(tmp, file)
}

/// Each command's latest run where that run failed: what is broken now. A success of one
/// command does not hide another's failure.
pub fn failing(runs: &[Run]) -> Vec<&Run> {
    let mut latest: Vec<&Run> = Vec::new();
    for r in runs {
        match latest.iter_mut().find(|l| l.what == r.what) {
            Some(l) => *l = r,
            None => latest.push(r),
        }
    }
    latest.retain(|r| !r.ok);
    latest
}

/// The plugin's kept runs, oldest first.
pub fn runs(plugins: &Path, id: &str) -> Vec<Run> {
    std::fs::read_to_string(crate::setup::data_dir(plugins, id).join("runs.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_dir(name: &str, manifest: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("kl-pctx-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("plugins").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plugin.json"), manifest).unwrap();
        std::fs::write(dir.join("hooks.json"), "{}").unwrap();
        (root, dir)
    }

    #[test]
    fn commands_get_the_plugin_and_its_settings_as_variables() {
        let (root, dir) = plugin_dir(
            "notify",
            r#"{"api":1,"id":"notify","name":"N","provides":["hooks"],"options":[
              {"key":"only_errors","title":"只在出错时","type":"boolean","default":false},
              {"key":"sound","title":"声音","type":"choice","choices":["Glass","Ping"],"default":"Glass"}]}"#,
        );
        let plugins = root.join("plugins");
        let get = |k: &str| env(&dir).into_iter().find(|(n, _)| n == k).map(|(_, v)| v);
        assert_eq!(get("MOORING_PLUGIN_ID").as_deref(), Some("notify"));
        assert_eq!(get("MOORING_OPTION_ONLY_ERRORS").as_deref(), Some("0"));
        assert_eq!(get("MOORING_OPTION_SOUND").as_deref(), Some("Glass"));
        assert!(get("MOORING_RUNTIME").is_some());
        set(&plugins, "notify", "only_errors", Value::Bool(true)).unwrap();
        assert_eq!(get("MOORING_OPTION_ONLY_ERRORS").as_deref(), Some("1"));
        assert!(set(&plugins, "notify", "sound", "Boom".into()).is_err());
        assert!(set(&plugins, "notify", "nope", Value::Bool(true)).is_err());
        // Back to the default: nothing stored for it.
        set(&plugins, "notify", "only_errors", Value::Bool(false)).unwrap();
        let stored = std::fs::read_to_string(root.join("plugin-data/notify/options.json")).unwrap();
        assert!(!stored.contains("only_errors"), "{stored}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_steady_command_logs_only_its_changes() {
        let (root, dir) = plugin_dir(
            "steady",
            r#"{"api":1,"id":"steady","name":"S","provides":["hooks"]}"#,
        );
        let plugins = root.join("plugins");
        let fail = Err("退出码 1".to_owned());
        for r in [Ok(()), Ok(()), fail.clone(), fail.clone(), Ok(()), Ok(())] {
            record(&dir, "decorations/x", &r, 3, true);
        }
        record(&dir, "setup", &Ok(()), 120, false);
        let got: Vec<(String, bool)> = runs(&plugins, "steady")
            .into_iter()
            .map(|r| (r.what, r.ok))
            .collect();
        assert_eq!(
            got,
            [
                ("decorations/x".to_owned(), false),
                ("decorations/x".to_owned(), true),
                ("setup".to_owned(), true),
            ]
        );
        for _ in 0..KEPT + 5 {
            record(&dir, "hooks/h", &Ok(()), 1, false);
        }
        assert_eq!(runs(&plugins, "steady").len(), KEPT);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn settings_are_checked() {
        let opt = |json: &str| -> Vec<Opt> { serde_json::from_str(json).unwrap() };
        assert!(
            check_options(&opt(
                r#"[{"key":"Bad","title":"x","type":"string","default":""}]"#
            ))
            .is_err()
        );
        assert!(
            check_options(&opt(
                r#"[{"key":"n","title":"x","type":"number","default":"3"}]"#
            ))
            .is_err()
        );
        assert!(
            check_options(&opt(
                r#"[{"key":"c","title":"x","type":"choice","default":"a"}]"#
            ))
            .is_err()
        );
        let ok = opt(r#"[{"key":"n","title":"间隔","type":"number","default":300}]"#);
        assert!(check_options(&ok).is_ok());
        assert_eq!(ok[0].parse("60").unwrap(), serde_json::json!(60.0));
        assert!(ok[0].parse("abc").is_err());
    }

    /// Hooks finish together, in the host and the app at once: no run is lost.
    #[test]
    fn runs_from_many_threads_are_all_kept() {
        let (root, dir) = plugin_dir(
            "conc",
            r#"{"api":1,"id":"conc","name":"C","provides":["hooks"]}"#,
        );
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    for i in 0..5 {
                        record(&dir, &format!("hooks/t{t}-{i}"), &Ok(()), 1, false);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(runs(&root.join("plugins"), "conc").len(), 40);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn another_commands_success_does_not_hide_a_failure() {
        let run = |what: &str, ok| Run {
            at: 1,
            what: what.into(),
            ok,
            detail: String::new(),
            ms: 1,
        };
        let all = [
            run("a", false),
            run("b", true),
            run("c", false),
            run("c", true),
        ];
        let broken: Vec<&str> = failing(&all).iter().map(|r| r.what.as_str()).collect();
        assert_eq!(broken, ["a"]);
    }

    /// A secret reaches the command and nothing else: not the file, not the settings page,
    /// not the run log even when the command prints it.
    #[test]
    fn a_secret_reaches_the_command_and_stays_out_of_files_and_logs() {
        let (root, dir) = plugin_dir(
            "sec",
            r#"{"api":1,"id":"sec","name":"S","provides":["hooks"],"options":[
              {"key":"token","title":"密钥","type":"string","default":"","secret":true}]}"#,
        );
        let plugins = root.join("plugins");
        let get = |k: &str| env(&dir).into_iter().find(|(n, _)| n == k).map(|(_, v)| v);
        assert_eq!(get("MOORING_OPTION_TOKEN").as_deref(), Some(""));
        set(&plugins, "sec", "token", "sk-very-secret".into()).unwrap();
        assert_eq!(
            get("MOORING_OPTION_TOKEN").as_deref(),
            Some("sk-very-secret")
        );
        let stored = std::fs::read_to_string(root.join("plugin-data/sec/options.json")).unwrap();
        assert!(!stored.contains("sk-very"), "{stored}");
        let p = crate::plugin::check(&dir).unwrap();
        assert_eq!(values(&plugins, &p)[0].1, Value::String(SECRET_SET.into()));
        record(
            &dir,
            "hooks/h",
            &Err("认证失败 sk-very-secret 不对".into()),
            1,
            false,
        );
        let log = std::fs::read_to_string(root.join("plugin-data/sec/runs.jsonl")).unwrap();
        assert!(!log.contains("sk-very") && log.contains("••••"), "{log}");
        // Empty takes it away.
        set(&plugins, "sec", "token", "".into()).unwrap();
        assert_eq!(get("MOORING_OPTION_TOKEN").as_deref(), Some(""));
        assert_eq!(crate::secret::get("sec", "token"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_secret_must_be_an_empty_string_setting() {
        let opt = |json: &str| -> Vec<Opt> { serde_json::from_str(json).unwrap() };
        for bad in [
            r#"[{"key":"k","title":"t","type":"boolean","default":false,"secret":true}]"#,
            r#"[{"key":"k","title":"t","type":"string","default":"x","secret":true}]"#,
        ] {
            assert!(check_options(&opt(bad)).unwrap_err().contains("secret"));
        }
    }
}
