//! Plugin packages: one directory per plugin under `<config_dir>/plugins`, each with a
//! `plugin.json` naming the kinds it provides (`Kind`). Every kind reads its files from
//! here, and every outcome lands in one report the settings page shows: which plugin and
//! file took effect, and why one did not.
//!
//! A bad plugin never stops the others or the app: it is reported and skipped. A plugin
//! written for a newer Mooring loads what this one understands: a kind or a manifest field
//! it does not know is skipped with a warning, and `mooring` (the plugin level it needs,
//! `LEVEL`) refuses it outright when this build is too old.
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The package format's major version; a plugin of another is refused, not guessed at.
pub const API: u32 = 1;
/// What this build's plugin support adds up to, raised whenever a kind or a manifest field
/// is added: 1 the first seven kinds, 2 `setup`, `quota`, `files` and `options`, 3 an
/// action's `agent` and a secret setting. A plugin needing a higher level says so in `mooring`, and is refused
/// with "update Mooring".
pub const LEVEL: u32 = 3;

/// What a plugin can extend. The file each kind reads is fixed, so a manifest only says
/// which kinds it means to provide and a missing file is reported rather than ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `agent.json`, and any `agents/*.json`: CLIs the registry learns (see `agent`).
    Agent,
    /// `actions.json`: commands for the pane menu and ⌘P (see `action`).
    Actions,
    /// `rules.json`: which commands count as verification (see `rule`).
    Rules,
    /// `sidebar.json`: ways to arrange the left column (see `sidebar`).
    Sidebar,
    /// `hooks.json`: commands or webhooks told when sessions change state (see `hook`).
    Hooks,
    /// `decorations.json`: short command output shown on each pane (see `decoration`).
    Decorations,
    /// `effects.json`: which animation 标记完成 plays (see `effect`).
    Effects,
    /// `setup.json`: commands run when it is put to use and taken away (see `setup`).
    Setup,
    /// `quota.json`: where an Agent's usage limits are read (see `quota`).
    Quota,
}
impl Kind {
    /// The kind a manifest names, if this build knows it.
    pub fn from_name(name: &str) -> Option<Kind> {
        serde_json::from_value(serde_json::Value::String(name.to_owned())).ok()
    }
    /// The plugin level that introduced it.
    pub fn level(self) -> u32 {
        match self {
            Self::Setup | Self::Quota => 2,
            _ => 1,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::Actions => "动作",
            Self::Rules => "验证规则",
            Self::Sidebar => "左栏",
            Self::Hooks => "Hooks",
            Self::Decorations => "窗格装饰",
            Self::Effects => "动效",
            Self::Setup => "安装步骤",
            Self::Quota => "额度",
        }
    }
    /// The files of this kind inside a plugin directory, in load order.
    pub fn files(self, dir: &Path) -> Vec<PathBuf> {
        match self {
            Self::Agent => {
                let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("agents"))
                    .map(|entries| {
                        entries
                            .filter_map(|e| e.ok().map(|e| e.path()))
                            .filter(|p| p.extension().is_some_and(|e| e == "json"))
                            .collect()
                    })
                    .unwrap_or_default();
                files.sort();
                let single = dir.join("agent.json");
                if single.is_file() {
                    files.insert(0, single);
                }
                files
            }
            Self::Actions => [dir.join("actions.json")]
                .into_iter()
                .filter(|p| p.is_file())
                .collect(),
            Self::Rules => [dir.join("rules.json")]
                .into_iter()
                .filter(|p| p.is_file())
                .collect(),
            Self::Hooks => [dir.join("hooks.json")]
                .into_iter()
                .filter(|p| p.is_file())
                .collect(),
            Self::Decorations => [dir.join("decorations.json")]
                .into_iter()
                .filter(|p| p.is_file())
                .collect(),
            Self::Effects => [dir.join("effects.json")]
                .into_iter()
                .filter(|p| p.is_file())
                .collect(),
            Self::Sidebar => [dir.join("sidebar.json")]
                .into_iter()
                .filter(|p| p.is_file())
                .collect(),
            Self::Setup => [dir.join("setup.json")]
                .into_iter()
                .filter(|p| p.is_file())
                .collect(),
            Self::Quota => [dir.join("quota.json")]
                .into_iter()
                .filter(|p| p.is_file())
                .collect(),
        }
    }
}

/// Names a newer Mooring may add to `provides` are kept as text, so this build can skip
/// them instead of refusing the plugin.
pub fn known_kinds<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Kind>, D::Error> {
    let names: Vec<String> = Deserialize::deserialize(d)?;
    Ok(names.iter().filter_map(|n| Kind::from_name(n)).collect())
}

/// Fields a manifest may carry; any other is skipped with a warning.
const MANIFEST_FIELDS: [&str; 11] = [
    "api",
    "id",
    "name",
    "version",
    "description",
    "provides",
    "files",
    "mooring",
    "options",
    "touches",
    "network",
];

#[derive(Deserialize)]
struct Manifest {
    api: u32,
    id: String,
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    provides: Vec<String>,
    /// The plugin level it needs (`LEVEL`); without it, what its kinds and fields need.
    #[serde(default)]
    mooring: Option<u32>,
    /// Its settings, shown in Settings › 插件 and handed to its commands.
    #[serde(default)]
    options: Vec<crate::plugin_context::Opt>,
    /// Files of other programs its setup changes, shown before it is installed.
    #[serde(default)]
    touches: Vec<String>,
    /// Whether its commands reach the network; shown before it is installed.
    #[serde(default)]
    network: Option<bool>,
    /// Other files it ships, such as the scripts its commands run, relative to its directory.
    #[serde(default)]
    files: Vec<String>,
}

/// A plugin whose manifest checked out.
#[derive(Clone, Debug, PartialEq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub dir: PathBuf,
    pub provides: Vec<Kind>,
    /// The other files it ships (`files` in its manifest).
    pub files: Vec<PathBuf>,
    /// The plugin level it needs: its `mooring`, else what its kinds and fields need.
    pub level: u32,
    /// What this build skipped: kinds and manifest fields it does not know.
    pub warnings: Vec<String>,
    /// Its settings (`options` in its manifest).
    pub options: Vec<crate::plugin_context::Opt>,
    /// Files of other programs it changes (`touches`), and whether it goes online.
    pub touches: Vec<String>,
    pub network: Option<bool>,
    /// Turned off in settings: listed, but none of its files load.
    pub enabled: bool,
}
impl Plugin {
    /// Its files of `kind`; none when it does not provide that kind or is turned off.
    pub fn files(&self, kind: Kind) -> Vec<PathBuf> {
        if self.enabled && self.provides.contains(&kind) {
            kind.files(&self.dir)
        } else {
            vec![]
        }
    }
}

/// One plugin directory's outcome; its files' own outcomes are reported per kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub dir: PathBuf,
    pub result: Result<Plugin, String>,
}

/// A value loaded once and replaced whole when plugins change. Readers get a `'static`
/// reference, so a replaced value is leaked rather than freed: replacing happens only on
/// an install, an uninstall or a switch, a few kilobytes each time.
pub struct Slot<T: 'static>(std::sync::RwLock<Option<&'static T>>);
impl<T: 'static> Slot<T> {
    pub const fn new() -> Self {
        Self(std::sync::RwLock::new(None))
    }
    pub fn get(&self) -> Option<&'static T> {
        *self.0.read().unwrap_or_else(|e| e.into_inner())
    }
    /// The value, loading it with `load` the first time.
    pub fn get_or_init(&self, load: impl FnOnce() -> T) -> &'static T {
        if let Some(v) = self.get() {
            return v;
        }
        let mut slot = self.0.write().unwrap_or_else(|e| e.into_inner());
        slot.get_or_insert_with(|| Box::leak(Box::new(load())))
    }
    /// Replaces the value from now on; references already handed out keep the old one.
    pub fn set(&self, value: T) -> &'static T {
        let value: &'static T = Box::leak(Box::new(value));
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = Some(value);
        value
    }
}
impl<T: 'static> Default for Slot<T> {
    fn default() -> Self {
        Self::new()
    }
}

static INSTALLED: Slot<Vec<Found>> = Slot::new();
static CONFIG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Discovers the plugins under `<config>/plugins` once; later calls return the same list
/// until `reload`.
pub fn install(config: &Path) -> &'static [Found] {
    let _ = CONFIG.set(config.to_owned());
    INSTALLED.get_or_init(|| {
        let plugins = dir(config);
        if plugins.is_dir() {
            crate::market::recover(&plugins);
        }
        discover(&plugins)
    })
}

/// The config directory `install` was given.
pub fn config() -> Option<&'static Path> {
    CONFIG.get().map(PathBuf::as_path)
}

/// Discovers the plugins again and reloads every kind. Does nothing before `install`.
pub fn reload() {
    let Some(config) = CONFIG.get() else {
        return;
    };
    INSTALLED.set(discover(&dir(config)));
    // First: the others name Agents.
    crate::agent::reload();
    crate::action::reload();
    crate::sidebar::reload();
    crate::decoration::reload();
    crate::effect::reload();
    crate::rule::reload();
    crate::hook::reload();
    crate::quota::reload();
}

/// The installed plugin with this id, if one loaded.
pub fn installed(id: &str) -> Option<&'static Plugin> {
    report()
        .iter()
        .filter_map(|f| f.result.as_ref().ok())
        .find(|p| p.id == id)
}

/// Held while plugins are installed, removed or switched: the app and a `mooring plugins` in
/// a terminal must not swap the same directory at once. Released when dropped.
pub(crate) fn lock(plugins: &Path) -> Result<std::fs::File, String> {
    use std::os::fd::AsRawFd;
    std::fs::create_dir_all(plugins).map_err(|e| format!("建不了插件目录：{e}"))?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(plugins.join(".lock"))
        .map_err(|e| format!("打不开插件锁：{e}"))?;
    // SAFETY: flock on a file this function just opened and returns.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(format!("拿不到插件锁：{}", std::io::Error::last_os_error()));
    }
    Ok(file)
}

/// Removes an installed plugin's directory and its switch; on success, anything you should
/// know about what is left. A plugin that does not load can be removed too, by its directory
/// name, since nothing else could ever remove it; its teardown cannot run, and that is said.
/// With `force`, a failing teardown no longer keeps it, and that is said too.
pub fn uninstall(id: &str, force: bool) -> Result<Option<String>, String> {
    let plugins = dir(CONFIG.get().ok_or("找不到配置目录")?);
    uninstall_in(&plugins, id, force)
}

pub fn uninstall_in(plugins: &Path, id: &str, force: bool) -> Result<Option<String>, String> {
    let _held = lock(plugins)?;
    let mut warnings = Vec::new();
    let found = discover(plugins);
    let loaded = found
        .iter()
        .find(|f| f.result.as_ref().is_ok_and(|p| p.id == id));
    let (target, data_id) = match loaded {
        Some(f) => {
            if f.dir.parent() != Some(plugins) {
                return Err(format!("{} 不在插件目录里，没有删", f.dir.display()));
            }
            // What it set up is undone first; when that fails it stays, since what it
            // changed may still need its files (a status line pointing at its script).
            let p = f.result.as_ref().map_err(Clone::clone)?;
            if p.enabled
                && let Err(why) = crate::setup::run(plugins, &f.dir, crate::setup::Phase::Teardown)
            {
                if !force {
                    return Err(format!("没有卸载：{why}"));
                }
                warnings.push(left_behind(&why, &p.touches));
            }
            (f.dir.clone(), id.to_owned())
        }
        None => {
            // Not loading: only an exact directory of the plugins folder, by name.
            let named = crate::market::safe_path(id)
                .then(|| {
                    found
                        .iter()
                        .find(|f| f.dir == plugins.join(id) && f.result.is_err())
                })
                .flatten();
            let Some(f) = named else {
                return Err(format!("没有装叫 {id} 的插件"));
            };
            if f.dir.join("setup.json").is_file() {
                warnings
                    .push("它加载不了，teardown 没有跑；它改过的别的程序的设置可能还在".to_owned());
            }
            (
                f.dir.clone(),
                declared_id(&f.dir).unwrap_or_else(|| id.to_owned()),
            )
        }
    };
    let secrets = secret_keys(&target);
    std::fs::remove_dir_all(&target).map_err(|e| format!("删不掉 {}：{e}", target.display()))?;
    for key in secrets {
        crate::secret::delete(&data_id, &key);
    }
    // What it kept is gone with it; teardown has already put back what needed putting.
    let data = crate::setup::data_dir(plugins, &data_id);
    if data.is_dir()
        && let Err(e) = std::fs::remove_dir_all(&data)
    {
        warnings.push(format!("它的数据目录 {} 没删掉：{e}", data.display()));
    }
    let mut off = disabled(plugins);
    if off.contains(&data_id) {
        off.retain(|d| *d != data_id);
        write_switches(plugins, &off).map_err(|e| format!("没能更新插件开关：{e}"))?;
    }
    Ok((!warnings.is_empty()).then(|| warnings.join("；")))
}

/// What a forced removal leaves: why teardown failed and what it says it had changed.
fn left_behind(why: &str, touches: &[String]) -> String {
    if touches.is_empty() {
        format!("{why}，它改过的东西可能还在")
    } else {
        format!("{why}，这些文件可能还留着它的改动：{}", touches.join("、"))
    }
}

/// The secret settings a manifest names, read loosely: a plugin that does not load may still
/// have left them in the keychain.
fn secret_keys(dir: &Path) -> Vec<String> {
    let Some(value) = std::fs::read(dir.join("plugin.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    else {
        return vec![];
    };
    value["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["secret"] == true)
        .filter_map(|o| o["key"].as_str().map(str::to_owned))
        .collect()
}

/// The id a manifest names, read loosely: for a plugin that does not load.
fn declared_id(dir: &Path) -> Option<String> {
    let bytes = std::fs::read(dir.join("plugin.json")).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value["id"]
        .as_str()
        .filter(|id| valid_id(id))
        .map(str::to_owned)
}

/// `plugins/disabled.json`: the ids turned off in settings, as a JSON list.
fn switches(plugins: &Path) -> PathBuf {
    plugins.join("disabled.json")
}
fn disabled(plugins: &Path) -> Vec<String> {
    std::fs::read(switches(plugins))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
/// Whether the plugin with this id is turned off in the plugins directory `plugins`.
pub fn turned_off(plugins: &Path, id: &str) -> bool {
    disabled(plugins).iter().any(|d| d == id)
}
/// Whether the plugin will load at the next start: what settings show its switch as.
pub fn enabled_next_start(id: &str) -> bool {
    CONFIG
        .get()
        .is_none_or(|c| !disabled(&dir(c)).iter().any(|d| d == id))
}
/// Turns a plugin on or off; `reload` applies it to the running process. A plugin whose
/// setup fails stays off, with what it managed undone as far as its teardown can. A failing
/// teardown keeps it on unless `force`: on, it still runs, so you must be able to stop it.
pub fn set_enabled(id: &str, on: bool, force: bool) -> Result<Option<String>, String> {
    let plugins = dir(CONFIG.get().ok_or("找不到配置目录")?);
    set_enabled_in(&plugins, id, on, force)
}

pub fn set_enabled_in(
    plugins: &Path,
    id: &str,
    on: bool,
    force: bool,
) -> Result<Option<String>, String> {
    use crate::setup::Phase;
    let _held = lock(plugins)?;
    let mut off = disabled(plugins);
    let was_on = !off.iter().any(|d| d == id);
    let mut warning = None;
    if was_on != on
        && let Some(f) = discover(plugins)
            .into_iter()
            .find(|f| f.result.as_ref().is_ok_and(|p| p.id == id))
    {
        if on {
            if let Err(why) = crate::setup::run(plugins, &f.dir, Phase::Setup) {
                let _ = crate::setup::run(plugins, &f.dir, Phase::Teardown);
                return Err(why);
            }
        } else if let Err(why) = crate::setup::run(plugins, &f.dir, Phase::Teardown) {
            if !force {
                return Err(why);
            }
            let touches = f.result.as_ref().map_or(&[][..], |p| p.touches.as_slice());
            warning = Some(left_behind(&why, touches));
        }
    }
    off.retain(|d| d != id);
    if !on {
        off.push(id.to_owned());
    }
    write_switches(plugins, &off).map_err(|e| format!("没能更新插件开关：{e}"))?;
    Ok(warning)
}

/// Marks it off without running anything: its setup just failed, so its teardown has had
/// its chance and turning it on later runs the setup again.
pub(crate) fn mark_off(plugins: &Path, id: &str) -> std::io::Result<()> {
    let mut off = disabled(plugins);
    if !off.iter().any(|d| d == id) {
        off.push(id.to_owned());
    }
    write_switches(plugins, &off)
}

fn write_switches(plugins: &Path, off: &[String]) -> std::io::Result<()> {
    std::fs::create_dir_all(plugins)?;
    let tmp = plugins.join(".disabled.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(off)?)?;
    std::fs::rename(tmp, switches(plugins))
}

/// Every plugin directory's outcome since `install`; empty before it.
pub fn report() -> &'static [Found] {
    INSTALLED.get().map_or(&[], Vec::as_slice)
}

/// The loaded plugins' files of one kind, plugin by plugin in directory order.
pub fn files(kind: Kind) -> Vec<PathBuf> {
    report()
        .iter()
        .filter_map(|f| f.result.as_ref().ok())
        .flat_map(|p| p.files(kind))
        .collect()
}

/// Why a declaration file does not parse. A field or value this build has never heard of
/// may be a typo or may be from a newer Mooring: say both.
pub fn bad_format(e: &serde_json::Error) -> String {
    let text = e.to_string();
    if text.contains("unknown field") || text.contains("unknown variant") {
        format!("格式不对：{text}（写错了，或者要更新 Mooring 才认识）")
    } else {
        format!("格式不对：{text}")
    }
}

/// One kind's files through the loader a running process reads them with.
pub fn load_kind(kind: Kind, files: Vec<PathBuf>, home: &Path) -> Vec<crate::agent::Loaded> {
    match kind {
        Kind::Agent => crate::agent::load_files(files, home).1,
        Kind::Actions => crate::action::load(files).1,
        Kind::Rules => crate::rule::load(files).1,
        Kind::Sidebar => crate::sidebar::load(files).1,
        Kind::Hooks => crate::hook::load(files).1,
        Kind::Decorations => crate::decoration::load(files).1,
        Kind::Effects => crate::effect::load(files).1,
        Kind::Setup => crate::setup::load(files),
        Kind::Quota => crate::quota::load(files).1,
    }
}

/// `<config_dir>/plugins`, the directory the settings page tells people to use.
pub fn dir(config: &Path) -> PathBuf {
    config.join("plugins")
}

/// Every plugin directory under `plugins`, by name; a later id already taken is refused.
pub fn discover(plugins: &Path) -> Vec<Found> {
    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(plugins) {
        Ok(entries) => entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            // Staging and replaced copies of an install in progress.
            .filter(|p| {
                !p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            })
            .collect(),
        Err(_) => return vec![],
    };
    dirs.sort();
    let off = disabled(plugins);
    let mut found: Vec<Found> = Vec::new();
    for dir in dirs {
        let mut result = read(&dir).map(|mut p| {
            p.enabled = !off.contains(&p.id);
            p
        });
        if let Ok(plugin) = &result
            && found
                .iter()
                .any(|f| f.result.as_ref().is_ok_and(|p| p.id == plugin.id))
        {
            result = Err(format!("id \"{}\" 和前面的插件重名", plugin.id));
        }
        found.push(Found { dir, result });
    }
    found
}

/// Reads one plugin directory as `discover` would, without registering it.
pub fn check(dir: &Path) -> Result<Plugin, String> {
    read(dir)
}

fn read(dir: &Path) -> Result<Plugin, String> {
    let file = dir.join("plugin.json");
    let bytes = std::fs::read(&file).map_err(|_| "缺少 plugin.json".to_owned())?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("plugin.json 格式不对：{e}"))?;
    let m: Manifest =
        serde_json::from_value(value.clone()).map_err(|e| format!("plugin.json 格式不对：{e}"))?;
    if m.api != API {
        return Err(format!("插件格式是 api {}，这个版本只认 api {API}", m.api));
    }
    if !valid_id(&m.id) {
        return Err(format!(
            "id \"{}\" 只能用小写字母、数字和 -，不超过 40 个字",
            m.id
        ));
    }
    if m.name.trim().is_empty() {
        return Err("name 不能为空".into());
    }
    if m.provides.is_empty() {
        return Err(
            "provides 至少写一种：agent、actions、rules、sidebar、hooks、decorations、effects、setup、quota"
                .into(),
        );
    }
    let mut warnings = Vec::new();
    let mut provides = Vec::new();
    for name in &m.provides {
        match Kind::from_name(name) {
            Some(k) if !provides.contains(&k) => provides.push(k),
            Some(_) => {}
            None => warnings.push(format!(
                "这个版本不认识「{name}」类，跳过了；更新 Mooring 后可用"
            )),
        }
    }
    if let Some(fields) = value.as_object() {
        for key in fields
            .keys()
            .filter(|k| !MANIFEST_FIELDS.contains(&k.as_str()))
        {
            warnings.push(format!("这个版本不认识 plugin.json 的「{key}」，忽略了"));
        }
    }
    let needed = provides
        .iter()
        .map(|k| k.level())
        .chain((!m.files.is_empty() || !m.options.is_empty()).then_some(2))
        .chain(m.options.iter().any(|o| o.secret).then_some(3))
        .max()
        .unwrap_or(1);
    let level = m.mooring.unwrap_or(needed).max(needed);
    if level > LEVEL {
        return Err(format!(
            "要更新 Mooring：它需要插件级别 {level}，这个版本是 {LEVEL}"
        ));
    }
    if provides.is_empty() {
        return Err(format!(
            "这个版本不认识它提供的任何一类（{}），要更新 Mooring",
            m.provides.join("、")
        ));
    }
    crate::plugin_context::check_options(&m.options)?;
    let mut files = Vec::new();
    for f in &m.files {
        if !crate::market::safe_path(f) {
            return Err(format!("files 里的 \"{f}\" 要是插件目录里的相对路径"));
        }
        let path = dir.join(f);
        if !path.is_file() {
            return Err(format!("files 里写了 {f}，但目录里没有它"));
        }
        files.push(path);
    }
    let plugin = Plugin {
        id: m.id,
        name: m.name,
        version: m.version,
        description: m.description,
        dir: dir.to_owned(),
        provides,
        files,
        level,
        warnings,
        options: m.options,
        touches: m.touches,
        network: m.network,
        enabled: true,
    };
    for kind in &plugin.provides {
        if kind.files(dir).is_empty() {
            return Err(format!("写了提供 {}，但目录里没有它的文件", kind.label()));
        }
    }
    Ok(plugin)
}

/// The id rule every plugin and declaration shares.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kl-plugin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn plugin(root: &Path, dir: &str, manifest: &str, files: &[(&str, &str)]) {
        let d = root.join(dir);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("plugin.json"), manifest).unwrap();
        for (f, body) in files {
            let p = d.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
    }

    #[test]
    fn a_bad_plugin_is_reported_and_the_rest_load() {
        let root = tmp("discover");
        plugin(
            &root,
            "a-aider",
            r#"{"api":1,"id":"aider","name":"Aider","provides":["agent"]}"#,
            &[("agent.json", "{}"), ("agents/extra.json", "{}")],
        );
        plugin(&root, "b-broken", "{not json", &[]);
        plugin(
            &root,
            "c-future",
            r#"{"api":2,"id":"future","name":"F","provides":["agent"]}"#,
            &[("agent.json", "{}")],
        );
        plugin(
            &root,
            "d-empty",
            r#"{"api":1,"id":"empty","name":"E","provides":["agent"]}"#,
            &[],
        );
        plugin(
            &root,
            "e-dup",
            r#"{"api":1,"id":"aider","name":"Again","provides":["agent"]}"#,
            &[("agent.json", "{}")],
        );
        std::fs::create_dir_all(root.join("f-nomanifest")).unwrap();
        let found = discover(&root);
        let results: Vec<_> = found
            .iter()
            .map(|f| f.result.as_ref().map(|p| p.id.clone()))
            .collect();
        assert_eq!(results[0], Ok("aider".into()));
        assert!(results[1].as_ref().unwrap_err().contains("格式不对"));
        assert!(results[2].as_ref().unwrap_err().contains("api 2"));
        assert!(results[3].as_ref().unwrap_err().contains("没有它的文件"));
        assert!(results[4].as_ref().unwrap_err().contains("重名"));
        assert!(
            results[5]
                .as_ref()
                .unwrap_err()
                .contains("缺少 plugin.json")
        );
        let aider = found[0].result.as_ref().unwrap();
        let files = aider.files(Kind::Agent);
        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("agent.json"), "agent.json loads first");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn shipped_example_plugins_load() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let found = discover(&root);
        assert!(found.len() >= 3);
        for f in &found {
            let p = f.result.as_ref().unwrap_or_else(|e| panic!("{e}: {f:?}"));
            let (_, actions) = crate::action::load(p.files(Kind::Actions));
            let (_, rules) = crate::rule::load(p.files(Kind::Rules));
            let (_, sidebar) = crate::sidebar::load(p.files(Kind::Sidebar));
            let (_, hooks) = crate::hook::load(p.files(Kind::Hooks));
            let (_, decorations) = crate::decoration::load(p.files(Kind::Decorations));
            let (_, effects) = crate::effect::load(p.files(Kind::Effects));
            let (_, quota) = crate::quota::load(p.files(Kind::Quota));
            let setup = crate::setup::load(p.files(Kind::Setup));
            for loaded in actions
                .iter()
                .chain(&rules)
                .chain(&sidebar)
                .chain(&hooks)
                .chain(&decorations)
                .chain(&effects)
                .chain(&quota)
                .chain(&setup)
            {
                assert!(loaded.result.is_ok(), "{loaded:?}");
            }
        }
    }

    #[test]
    fn a_turned_off_plugin_is_listed_but_loads_nothing() {
        let root = tmp("switch");
        plugin(
            &root,
            "a-aider",
            r#"{"api":1,"id":"aider","name":"Aider","provides":["agent"]}"#,
            &[("agent.json", "{}")],
        );
        std::fs::write(root.join("disabled.json"), r#"["aider"]"#).unwrap();
        let found = discover(&root);
        assert_eq!(found.len(), 1, "the switch file is not a plugin");
        let p = found[0].result.as_ref().unwrap();
        assert!(!p.enabled);
        assert!(p.files(Kind::Agent).is_empty());
        std::fs::write(root.join("disabled.json"), "not json").unwrap();
        assert!(discover(&root)[0].result.as_ref().unwrap().enabled);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_plugins_directory_is_no_plugins() {
        assert!(discover(Path::new("/nonexistent/mooring/plugins")).is_empty());
    }

    #[test]
    fn a_plugin_for_a_newer_mooring_loads_what_this_one_knows() {
        let root = tmp("newer");
        plugin(
            &root,
            "a-mixed",
            r#"{"api":1,"id":"mixed","name":"M","provides":["actions","canvas"],"homepage":"x"}"#,
            &[("actions.json", "{}")],
        );
        plugin(
            &root,
            "b-only-new",
            r#"{"api":1,"id":"only-new","name":"N","provides":["canvas"]}"#,
            &[],
        );
        plugin(
            &root,
            "c-level",
            r#"{"api":1,"id":"level","name":"L","provides":["actions"],"mooring":99}"#,
            &[("actions.json", "{}")],
        );
        plugin(
            &root,
            "d-quota",
            r#"{"api":1,"id":"quota","name":"Q","provides":["quota"]}"#,
            &[("quota.json", "{}")],
        );
        let found = discover(&root);
        let mixed = found[0].result.as_ref().unwrap();
        assert_eq!(mixed.provides, [Kind::Actions]);
        assert_eq!(mixed.warnings.len(), 2, "{:?}", mixed.warnings);
        assert!(mixed.warnings[0].contains("canvas"));
        assert!(mixed.warnings[1].contains("homepage"));
        assert_eq!(mixed.level, 1);
        assert!(
            found[1]
                .result
                .as_ref()
                .unwrap_err()
                .contains("更新 Mooring")
        );
        assert!(
            found[2]
                .result
                .as_ref()
                .unwrap_err()
                .contains("插件级别 99")
        );
        assert_eq!(
            found[3].result.as_ref().unwrap().level,
            2,
            "quota came in level 2"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    fn with_setup(root: &Path, id: &str, setup: &str, teardown: &str) {
        plugin(
            root,
            id,
            &format!(
                r#"{{"api":1,"id":"{id}","name":"x","provides":["setup"],"touches":["~/.x"]}}"#
            ),
            &[(
                "setup.json",
                &format!(r#"{{"api":1,"setup":"{setup}","teardown":"{teardown}"}}"#),
            )],
        );
    }

    /// A broken teardown must not trap a plugin: asked plainly it stays and says why, forced
    /// it goes and says what may be left.
    #[test]
    fn a_failing_teardown_can_be_overruled() {
        let config = tmp("stuck");
        let plugins = dir(&config);
        std::fs::create_dir_all(&plugins).unwrap();
        with_setup(&plugins, "stuck", "true", "exit 3");
        let err = set_enabled_in(&plugins, "stuck", false, false).unwrap_err();
        assert!(err.contains("退出码 3"), "{err}");
        assert!(!turned_off(&plugins, "stuck"));
        let warned = set_enabled_in(&plugins, "stuck", false, true)
            .unwrap()
            .unwrap();
        assert!(warned.contains("~/.x"), "{warned}");
        assert!(turned_off(&plugins, "stuck"));
        // Back on: its setup runs, and uninstalling meets the same teardown.
        assert_eq!(set_enabled_in(&plugins, "stuck", true, false), Ok(None));
        assert!(
            uninstall_in(&plugins, "stuck", false)
                .unwrap_err()
                .contains("没有卸载")
        );
        assert!(plugins.join("stuck").exists());
        let warned = uninstall_in(&plugins, "stuck", true).unwrap().unwrap();
        assert!(warned.contains("~/.x"), "{warned}");
        assert!(!plugins.join("stuck").exists());
        let _ = std::fs::remove_dir_all(&config);
    }

    /// A plugin that does not load has nothing else that could ever remove it.
    #[test]
    fn a_plugin_that_does_not_load_can_be_removed_by_its_directory() {
        let config = tmp("broken-remove");
        let plugins = dir(&config);
        plugin(
            &plugins,
            "broken",
            r#"{"api":1,"id":"broken-id","name":"x","provides":["hooks"],"mooring":99}"#,
            &[("setup.json", "{}")],
        );
        let data = crate::setup::data_dir(&plugins, "broken-id");
        std::fs::create_dir_all(&data).unwrap();
        for bad in ["../broken", "nothing", ".lock"] {
            assert!(uninstall_in(&plugins, bad, false).is_err(), "{bad}");
        }
        let warned = uninstall_in(&plugins, "broken", false).unwrap().unwrap();
        assert!(warned.contains("teardown"), "{warned}");
        assert!(!plugins.join("broken").exists() && !data.exists());
        let _ = std::fs::remove_dir_all(&config);
    }

    /// A setup that fails on the way on leaves it off and undoes what it did.
    #[test]
    fn a_setup_that_fails_when_turned_on_leaves_it_off() {
        let config = tmp("on-fails");
        let plugins = dir(&config);
        std::fs::create_dir_all(&plugins).unwrap();
        let mark = config.join("undone");
        with_setup(
            &plugins,
            "p",
            "exit 9",
            &format!("touch {}", mark.display()),
        );
        std::fs::write(switches(&plugins), r#"["p"]"#).unwrap();
        assert!(
            set_enabled_in(&plugins, "p", true, false)
                .unwrap_err()
                .contains("退出码 9")
        );
        assert!(turned_off(&plugins, "p"));
        assert!(mark.exists());
        let _ = std::fs::remove_dir_all(&config);
    }

    /// Taking a plugin away takes its secrets out of the keychain too.
    #[test]
    fn uninstalling_removes_its_secrets() {
        let config = tmp("secret-gone");
        let plugins = dir(&config);
        plugin(
            &plugins,
            "s",
            r#"{"api":1,"id":"s","name":"S","provides":["hooks"],"options":[
              {"key":"token","title":"t","type":"string","default":"","secret":true}]}"#,
            &[(
                "hooks.json",
                r#"{"api":1,"hooks":[{"id":"h","on":["done"],"command":"true"}]}"#,
            )],
        );
        crate::plugin_context::set(&plugins, "s", "token", "abc".into()).unwrap();
        assert_eq!(crate::secret::get("s", "token").as_deref(), Some("abc"));
        assert!(check(&plugins.join("s")).unwrap().level >= 3);
        uninstall_in(&plugins, "s", false).unwrap();
        assert_eq!(crate::secret::get("s", "token"), None);
        let _ = std::fs::remove_dir_all(&config);
    }
}
