//! The plugin catalog: plugins offered for one-click install, from two places that share
//! one index format.
//!
//! - **Bundled**: the app ships `plugins/` (copied into `Contents/Resources/plugins`),
//!   so its example plugins are installable offline.
//! - **Remote**: an `index.json` at a URL (by default this repository's `plugins/`
//!   on GitHub; `<config>/market` or `MOORING_PLUGIN_MARKET` names another). Files are
//!   fetched next to it as `<id>/<path>`, with the system curl.
//!
//! ```json
//! {"api": 1, "plugins": [{"id": "pane-facts", "name": "窗格信息", "version": "0.1.0",
//!   "description": "…", "provides": ["decorations"],
//!   "files": [{"path": "plugin.json", "sha256": "…", "size": 312}]}]}
//! ```
//!
//! **Who published it.** A catalog may carry `index.sig` beside `index.json`: an ed25519
//! signature over the index's exact bytes, whose file hashes in turn pin every file. So a
//! valid signature says one key published this whole catalog. This repository's key is built
//! in (`OFFICIAL_KEYS`) and its catalog must be signed by one of them; another catalog's key is shown
//! the first time and remembered with what was installed, so a change of publisher is asked
//! about; a catalog with no signature is allowed and says so; a bad signature is refused.
//!
//! **Versions.** An entry may list older versions (`versions`, each with its own `dir`), so
//! a build too old for the newest still gets the newest it can run.
//!
//! Installing is two steps, so what you are asked comes from the package itself: `prepare`
//! copies exactly the listed files, each checked against its sha256, into a staging
//! directory and reads them with the loaders the app uses (`Prepared` says what it will run
//! and what it adds over the installed version); `commit` swaps it in as `plugins/<id>` and,
//! when it has one, runs its `setup`. Copying and checking run nothing.
use crate::plugin::{self, Kind, Plugin};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

#[path = "market_install.rs"]
mod install;
#[path = "market_sign.rs"]
mod sign;
pub use install::*;
pub use sign::*;

/// The index format's major version.
pub const API: u32 = 1;
/// This repository's plugins, as published on GitHub.
pub const DEFAULT_REMOTE: &str =
    "https://raw.githubusercontent.com/majiayu000/fibo/main/plugins/index.json";
/// No plugin file is this large; a bigger one is refused, not written.
const MAX_FILE: u64 = 16 << 20;
/// The public halves of the keys this repository's catalog may be signed with. The private
/// halves are GitHub secrets, used only by `.github/workflows/sign-catalog.yml`. The first
/// signs today; the second is built in unused, so apps already out there keep trusting the
/// catalog if the first ever has to be retired.
pub const OFFICIAL_KEYS: [&str; 2] = [
    "e88de1d56240c9dbf6f150dfdc58347a967d78ed88dc07218cb116c976aa8988",
    "c4382767802055283d1ce6b2729adb5f5adf8517290cea36852a4e389279ccd5",
];

/// Fields a newer Mooring adds to the index are ignored, and kinds it does not know are
/// dropped from `provides`, so an older build still lists what it can install.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub api: u32,
    pub plugins: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(deserialize_with = "plugin::known_kinds")]
    pub provides: Vec<Kind>,
    /// The plugin level it needs (`plugin::LEVEL`), when above 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mooring: Option<u32>,
    /// Files of other programs it changes, and whether it goes online, as it declares.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touches: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<bool>,
    pub files: Vec<FileEntry>,
    /// Where its files are under `<id>/`, for an older version kept beside the newest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Older versions, newest first, each a complete entry: what a build too old for this
    /// one installs instead. A build from before these existed ignores them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

/// Where an offer's files come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The catalog directory that holds `index.json` and `<id>/` folders.
    Bundled(PathBuf),
    /// The URL directory that holds `index.json`, without the trailing slash.
    Remote(String),
}

/// One installable plugin and where to get it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub entry: Entry,
    pub source: Source,
    pub publisher: Publisher,
}

/// What a plugin's kinds may do once installed, in words for the install confirmation.
/// Copying it in runs nothing; its `setup`, if it has one, runs right after.
pub fn permissions(provides: &[Kind]) -> Vec<&'static str> {
    let mut out = Vec::new();
    for k in provides {
        out.push(match k {
            Kind::Actions => t!("右键菜单或 ⌘P 里点了它，就在终端里运行它写的命令"),
            Kind::Hooks => t!("会话等你、出错、做完或命令结束时，在后台运行它写的命令或发 webhook"),
            Kind::Decorations => t!("定时在窗格所在目录里运行它写的命令，显示第一行输出"),
            Kind::Agent => t!("读这个 Agent 的对话记录，并能用它写的命令启动和恢复会话"),
            Kind::Rules => t!("只决定哪些命令算跑过验证，不运行任何东西"),
            Kind::Sidebar => t!("只决定左栏怎么排；WASM 排法在沙盒里跑，读不到文件和网络"),
            Kind::Effects => t!("只挑选标记完成时播哪种动画，不运行任何东西"),
            Kind::Setup => t!("装上、更新或打开时运行它的 setup 命令，停用或卸载时运行 teardown 撤回，可以改别的程序的设置"),
            Kind::Quota => t!("读它指定的文件或定时运行它写的命令，取某个 Agent 的额度"),
            Kind::Themes => t!("只给设置 › 外观多几套主题，不运行任何东西"),
        });
    }
    out
}
/// Whether installing it lets it run commands on this computer: worth a confirmation.
pub fn runs_commands(provides: &[Kind]) -> bool {
    provides.iter().any(|k| {
        matches!(
            k,
            Kind::Actions
                | Kind::Hooks
                | Kind::Decorations
                | Kind::Agent
                | Kind::Setup
                | Kind::Quota
        )
    })
}

/// `1.10.0` is newer than `1.9.2`, as semver orders them: a pre-release (`1.0.0-beta`)
/// comes before its release, and build metadata (`+abc`) does not count.
pub fn newer(a: &str, b: &str) -> bool {
    use std::cmp::Ordering;
    fn split(v: &str) -> (Vec<u64>, Option<Vec<&str>>) {
        let v = v.trim().trim_start_matches('v');
        let v = v.split_once('+').map_or(v, |(v, _)| v);
        let (core, pre) = match v.split_once('-') {
            Some((core, pre)) => (core, Some(pre.split('.').collect())),
            None => (v, None),
        };
        (
            core.split('.').map(|p| p.parse().unwrap_or(0)).collect(),
            pre,
        )
    }
    let ((ca, pa), (cb, pb)) = (split(a), split(b));
    let width = ca.len().max(cb.len());
    let pad = |c: &[u64]| {
        (0..width)
            .map(|i| c.get(i).copied().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    match pad(&ca).cmp(&pad(&cb)) {
        Ordering::Equal => (),
        o => return o == Ordering::Greater,
    }
    match (pa, pb) {
        (None, None) => false,
        // The release is newer than any of its pre-releases.
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (Some(x), Some(y)) => {
            for (p, q) in x.iter().zip(&y) {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(p), Ok(q)) => p.cmp(&q),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => p.cmp(q),
                };
                if o != Ordering::Equal {
                    return o == Ordering::Greater;
                }
            }
            x.len() > y.len()
        }
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A listed path that stays inside the plugin's directory: relative, no `..`, no hidden
/// segments, no backslashes.
pub(crate) fn safe_path(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty()
        && !path.contains('\\')
        && p.components().all(|c| match c {
            Component::Normal(s) => !s.to_string_lossy().starts_with('.'),
            _ => false,
        })
}

/// What installing it means, for the confirmation: what its kinds may do, then the files
/// of other programs it says it changes and whether it goes online. The last two are the
/// plugin's own word; nothing enforces them.
pub fn disclosure(provides: &[Kind], touches: &[String], network: Option<bool>) -> Vec<String> {
    let mut out: Vec<String> = permissions(provides)
        .into_iter()
        .map(str::to_owned)
        .collect();
    if !touches.is_empty() {
        out.push(t!(
            "会改别的程序的文件：{files}",
            files = touches.join(t!("、"))
        ));
    }
    match network {
        Some(true) => out.push(t!("会联网").into()),
        Some(false) if runs_commands(provides) => out.push(t!("声明不联网").into()),
        _ => {}
    }
    out
}

impl Entry {
    /// Whether this build can run this version.
    pub fn compatible(&self) -> bool {
        self.mooring.is_none_or(|l| l <= plugin::LEVEL)
    }
    /// This version and every older one, newest first; each without its own `versions`.
    fn all_versions(&self) -> Vec<Entry> {
        let mut own = self.clone();
        own.versions = vec![];
        let mut all = vec![own];
        all.extend(self.versions.iter().cloned());
        all.sort_by(|a, b| {
            if newer(&a.version, &b.version) {
                std::cmp::Ordering::Less
            } else if newer(&b.version, &a.version) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        all
    }
    /// The newest version this build can run, carrying the rest for `Offer::pick`. With none
    /// it stays the newest, for `check` to say that Mooring needs updating.
    fn for_this_build(&self) -> Entry {
        let all = self.all_versions();
        let chosen = all.iter().find(|e| e.compatible()).unwrap_or(&all[0]);
        Entry {
            versions: all
                .iter()
                .filter(|e| e.version != chosen.version)
                .cloned()
                .collect(),
            ..chosen.clone()
        }
    }
    /// Refuses an entry that could not be installed safely, before fetching anything.
    pub fn check(&self) -> Result<(), String> {
        if !plugin::valid_id(&self.id) {
            return Err(t!("id \"{id}\" 不合法", id = self.id));
        }
        if let Some(level) = self.mooring.filter(|l| *l > plugin::LEVEL) {
            return Err(t!(
                "要更新 Fibo 才能装 {id}：它需要插件级别 {level}，这个版本是 {cur}",
                id = self.id,
                level,
                cur = plugin::LEVEL
            ));
        }
        if let Some(dir) = self.dir.as_deref().filter(|d| !safe_path(d)) {
            return Err(t!("{id} 的目录 \"{dir}\" 不安全", id = self.id, dir));
        }
        if !self.files.iter().any(|f| f.path == "plugin.json") {
            return Err(t!("{id} 的文件里没有 plugin.json", id = self.id));
        }
        for f in &self.files {
            if !safe_path(&f.path) {
                return Err(t!(
                    "{id} 的文件路径 \"{path}\" 不安全",
                    id = self.id,
                    path = f.path
                ));
            }
            if f.size > MAX_FILE {
                return Err(t!(
                    "{id} 的 {path} 太大（{size} 字节）",
                    id = self.id,
                    path = f.path,
                    size = f.size
                ));
            }
            if f.sha256.len() != 64 || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(t!(
                    "{id} 的 {path} 没有正确的 sha256",
                    id = self.id,
                    path = f.path
                ));
            }
        }
        Ok(())
    }
}

/// The files a plugin directory ships: `plugin.json`, every file of the kinds it provides,
/// and the `.wasm` files its sidebar presets name. Build files and sources stay behind.
pub fn shipped_files(plugin: &Plugin) -> Vec<PathBuf> {
    let mut files = vec![plugin.dir.join("plugin.json")];
    for kind in &plugin.provides {
        files.extend(kind.files(&plugin.dir));
    }
    files.extend(plugin.files.iter().cloned());
    if plugin.provides.contains(&Kind::Sidebar)
        && let Ok(bytes) = std::fs::read(plugin.dir.join("sidebar.json"))
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
    {
        for preset in v["presets"].as_array().into_iter().flatten() {
            if let Some(w) = preset["wasm"].as_str().filter(|w| safe_path(w)) {
                files.push(plugin.dir.join(w));
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Builds the index of every plugin directory under `dir`; a directory that does not
/// check out as a plugin is an error, so a broken example cannot be published.
pub fn index_of(dir: &Path) -> Result<Index, String> {
    let mut plugins = Vec::new();
    for found in plugin::discover(dir) {
        let p = found
            .result
            .map_err(|e| t!("{display}：{e}", display = found.dir.display(), e))?;
        if found.dir.file_name() != Some(std::ffi::OsStr::new(&p.id)) {
            return Err(t!(
                "{dir}：目录名要和 id「{id}」一样，安装时按 id 找文件",
                dir = found.dir.display(),
                id = p.id
            ));
        }
        let mut files = Vec::new();
        for file in shipped_files(&p) {
            let bytes = std::fs::read(&file)
                .map_err(|e| t!("读不了 {display}：{e}", display = file.display(), e))?;
            let path = file
                .strip_prefix(&p.dir)
                .map_err(|_| t!("{file} 不在插件目录里", file = file.display()))?
                .to_string_lossy()
                .into_owned();
            files.push(FileEntry {
                path,
                sha256: sha256(&bytes),
                size: bytes.len() as u64,
            });
        }
        let mut entry = entry_of(p, files);
        entry.versions = older_versions(&found.dir, &entry.id)?;
        plugins.push(entry);
    }
    Ok(Index { api: API, plugins })
}

fn entry_of(p: Plugin, files: Vec<FileEntry>) -> Entry {
    Entry {
        id: p.id,
        name: p.name,
        version: p.version.unwrap_or_else(|| "0.0.0".into()),
        description: p.description,
        provides: p.provides,
        mooring: (p.level > 1).then_some(p.level),
        touches: p.touches,
        network: p.network,
        files,
        dir: None,
        versions: vec![],
    }
}

/// The versions kept under `<plugin>/versions/<version>/`, newest first: each a complete
/// plugin directory, listed like the newest one.
fn older_versions(dir: &Path, id: &str) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir.join("versions"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = e.file_name().to_string_lossy().into_owned();
        if !e.path().is_dir() || name.starts_with('.') {
            continue;
        }
        let p = plugin::check(&e.path())
            .map_err(|why| t!("{display}：{e}", display = e.path().display(), e = why))?;
        if p.id != id {
            return Err(t!(
                "{dir}：id 要和插件一样（{id}）",
                dir = e.path().display(),
                id
            ));
        }
        let mut files = Vec::new();
        for file in shipped_files(&p) {
            let bytes = std::fs::read(&file)
                .map_err(|e| t!("读不了 {display}：{e}", display = file.display(), e))?;
            files.push(FileEntry {
                path: file
                    .strip_prefix(&p.dir)
                    .map_err(|_| t!("{file} 不在插件目录里", file = file.display()))?
                    .to_string_lossy()
                    .into_owned(),
                sha256: sha256(&bytes),
                size: bytes.len() as u64,
            });
        }
        let mut entry = entry_of(p, files);
        entry.dir = Some(format!("versions/{name}"));
        out.push(entry);
    }
    out.sort_by(|a: &Entry, b: &Entry| {
        if newer(&a.version, &b.version) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    Ok(out)
}

/// The index as `index.json` holds it: pretty, one trailing newline.
pub fn render(index: &Index) -> String {
    serde_json::to_string_pretty(index).unwrap_or_default() + "\n"
}

fn parse(bytes: &[u8]) -> Result<Index, String> {
    let index: Index =
        serde_json::from_slice(bytes).map_err(|e| t!("index.json 格式不对：{e}", e))?;
    if index.api != API {
        return Err(t!(
            "插件目录格式是 api {got}，这个版本只认 api {API}",
            got = index.api,
            API
        ));
    }
    Ok(index)
}

/// The catalog shipped with this build: `MOORING_PLUGIN_CATALOG`, else the bundle's
/// `Contents/Resources/plugins`, else `plugins` beside a development build.
pub fn bundled_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("MOORING_PLUGIN_CATALOG") {
        return Some(PathBuf::from(dir));
    }
    catalog_beside(&std::env::current_exe().ok()?)
}

/// The catalog that ships with the program at `exe`. Through a link on PATH (Homebrew's
/// `/opt/homebrew/bin/fibo`) `current_exe` names the link, whose folder has no catalog, so
/// the link is followed to the program inside the app first.
fn catalog_beside(exe: &Path) -> Option<PathBuf> {
    let exe = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_owned());
    let mac = exe.parent()?;
    let candidates = [
        mac.parent().map(|c| c.join("Resources/plugins")),
        // target/<profile>/mooring-app → plugins
        mac.parent()
            .and_then(Path::parent)
            .map(|n| n.join("plugins")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|d| d.join("index.json").is_file())
}

/// The bundled catalog's offers; none when this build ships no catalog.
pub fn bundled() -> Result<Vec<Offer>, String> {
    let Some(dir) = bundled_dir() else {
        return Ok(vec![]);
    };
    let bytes = std::fs::read(dir.join("index.json")).map_err(|e| {
        t!(
            "读不了 {display}：{e}",
            display = dir.join("index.json").display(),
            e
        )
    })?;
    Ok(parse(&bytes)?
        .plugins
        .into_iter()
        .map(|entry| Offer {
            entry: entry.for_this_build(),
            source: Source::Bundled(dir.clone()),
            // It came inside the app: as trusted as the app itself.
            publisher: Publisher::Official,
        })
        .collect())
}

/// The remote index's URL: `MOORING_PLUGIN_MARKET`, else the first line of
/// `<config>/market`, else this repository's.
pub fn remote_url() -> String {
    if let Some(url) = std::env::var("MOORING_PLUGIN_MARKET")
        .ok()
        .filter(|u| !u.trim().is_empty())
    {
        return url.trim().to_owned();
    }
    crate::agent::config_dir()
        .and_then(|d| std::fs::read_to_string(d.join("market")).ok())
        .and_then(|t| t.lines().next().map(|l| l.trim().to_owned()))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| DEFAULT_REMOTE.to_owned())
}

fn curl(url: &str) -> Result<Vec<u8>, String> {
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "20", "--max-filesize"])
        .arg(MAX_FILE.to_string())
        .arg("--proto")
        .arg("=https,file")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| t!("运行 curl 失败：{e}", e))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let code = format!("{:?}", out.status.code());
        return Err(if why.is_empty() {
            t!("下载 {url} 失败（curl 退出码 {code}）", url, code)
        } else {
            t!("下载 {url} 失败：{why}", url, why)
        });
    }
    Ok(out.stdout)
}

/// Fetches the remote index. Only `https://` and `file://` URLs are followed.
pub fn fetch_remote(url: &str) -> Result<Vec<Offer>, String> {
    let base = url
        .rsplit_once('/')
        .map(|(b, _)| b.to_owned())
        .ok_or_else(|| t!("{url} 不是一个地址", url))?;
    let bytes = curl(url)?;
    // Any failure to get a signature is no signature; this repository's address and the
    // pin on what is installed are what make that matter.
    let signature = curl(&format!("{base}/index.sig")).ok();
    let publisher = publisher_of(url, &bytes, signature.as_deref())?;
    let index = parse(&bytes)?;
    Ok(index
        .plugins
        .into_iter()
        .map(|entry| Offer {
            entry: entry.for_this_build(),
            source: Source::Remote(base.clone()),
            publisher: publisher.clone(),
        })
        .collect())
}

/// The bundled offers, each replaced by the remote one when that is newer, then the
/// plugins only the remote index has.
pub fn merge(bundled: Vec<Offer>, remote: Vec<Offer>) -> Vec<Offer> {
    let mut out = bundled;
    for r in remote {
        match out.iter_mut().find(|o| o.entry.id == r.entry.id) {
            // A newer version this build cannot run does not push out one it can.
            Some(o)
                if newer(&r.entry.version, &o.entry.version)
                    && (r.entry.compatible() || !o.entry.compatible()) =>
            {
                *o = r
            }
            Some(_) => {}
            None => out.push(r),
        }
    }
    out
}

impl Offer {
    fn fetch(&self, path: &str) -> Result<Vec<u8>, String> {
        match &self.source {
            Source::Bundled(dir) => {
                let mut file = dir.join(&self.entry.id);
                if let Some(version) = &self.entry.dir {
                    file.push(version);
                }
                file.push(path);
                std::fs::read(&file)
                    .map_err(|e| t!("读不了 {display}：{e}", display = file.display(), e))
            }
            Source::Remote(base) => {
                let id = &self.entry.id;
                curl(&match &self.entry.dir {
                    Some(dir) => format!("{base}/{id}/{dir}/{path}"),
                    None => format!("{base}/{id}/{path}"),
                })
            }
        }
    }
    /// This offer at a named version, if the catalog has it.
    pub fn pick(&self, version: &str) -> Option<Offer> {
        let entry = self
            .entry
            .all_versions()
            .into_iter()
            .find(|e| e.version == version)?;
        Some(Offer {
            entry,
            ..self.clone()
        })
    }
    /// Who it comes from, for comparing with what is installed: its publisher when signed,
    /// so a move to another address is no change; its address when not.
    fn origin_key(&self) -> String {
        match (&self.publisher, &self.source) {
            (Publisher::Unsigned, Source::Remote(base)) => base.clone(),
            (Publisher::Unsigned, Source::Bundled(_)) | (Publisher::Official, _) => {
                OFFICIAL.to_owned()
            }
            (publisher, _) => publisher.said(),
        }
    }
    /// Where it comes from, in words.
    pub fn origin(&self) -> &'static str {
        match self.source {
            Source::Bundled(_) => t!("随 App 附带"),
            Source::Remote(_) => t!("远程目录"),
        }
    }
    /// Fetches and checks it into a staging directory without touching what is installed.
    /// A package that says more than its listing, or whose files the loaders refuse, stops
    /// here: an update can never replace a working plugin with one that does not load.
    pub fn prepare(&self, plugins: &Path) -> Result<Prepared, String> {
        let e = &self.entry;
        e.check()?;
        std::fs::create_dir_all(plugins).map_err(|e| t!("建不了插件目录：{e}", e))?;
        static STAGES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = STAGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stage = plugins.join(format!(".installing-{}-{}-{n}", e.id, std::process::id()));
        // Dropped on every way out below: a refused package leaves nothing behind.
        let mut prepared = Prepared {
            stage: Some(stage.clone()),
            plugins: plugins.to_owned(),
            source: InstalledFrom {
                from: self.origin_key(),
                files: e.files.iter().map(|f| f.path.clone()).collect(),
                key: self.publisher.key(),
            },
            id: e.id.clone(),
            disclosure: vec![],
            added: vec![],
            replacing: vec![],
            must_ask: false,
        };
        for f in &e.files {
            let bytes = self.fetch(&f.path)?;
            if bytes.len() as u64 != f.size || sha256(&bytes) != f.sha256 {
                return Err(t!(
                    "{id} 的 {path} 和目录里登记的不一致（大小或 sha256 不对），没有安装",
                    id = e.id,
                    path = f.path
                ));
            }
            let to = stage.join(&f.path);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            std::fs::write(&to, bytes).map_err(|err| err.to_string())?;
        }
        let p = plugin::check(&stage)?;
        if p.id != e.id {
            return Err(t!(
                "plugin.json 里的 id 是 {got}，目录里登记的是 {expected}",
                got = p.id,
                expected = e.id
            ));
        }
        // What you are shown may never be less than what the package says of itself.
        if let Some(k) = p.provides.iter().find(|k| !e.provides.contains(k)) {
            return Err(t!(
                "{id} 的 plugin.json 声明了 {kind}，目录里没有登记，没有安装",
                id = e.id,
                kind = k.label()
            ));
        }
        if let Some(t) = p.touches.iter().find(|t| !e.touches.contains(t)) {
            return Err(t!(
                "{id} 的 plugin.json 说会改 {t}，目录里没有登记，没有安装",
                id = e.id,
                t
            ));
        }
        if p.network == Some(true) && e.network != Some(true) {
            return Err(t!(
                "{id} 的 plugin.json 说会联网，目录里没有登记，没有安装",
                id = e.id
            ));
        }
        loadable(&p)?;
        no_conflict(plugins, &p)?;
        let old = plugin::discover(plugins)
            .into_iter()
            .find_map(|f| f.result.ok().filter(|o| o.id == e.id));
        prepared.disclosure = disclosure_of(&p);
        prepared.added = old.as_ref().map(|o| added(o, &p)).unwrap_or_default();
        prepared.replacing = old
            .as_ref()
            .map(|o| replacing(plugins, o, &p, e, self))
            .unwrap_or_default();
        // Nothing vouches for a first install from outside: say who it is, or that nobody says.
        if old.is_none() {
            prepared.replacing.extend(match &self.publisher {
                Publisher::Official => None,
                Publisher::Signed { .. } => Some(t!(
                    "发布者：{who}，第一次见到这个密钥，以后换了会问你",
                    who = self.publisher.said()
                )),
                Publisher::Unsigned => {
                    Some(t!("这个目录没有签名，不能确认文件是谁发的").to_owned())
                }
            });
        }
        prepared.must_ask = !prepared.replacing.is_empty()
            || match &old {
                None => runs_commands(&p.provides),
                Some(_) => !prepared.added.is_empty(),
            };
        Ok(prepared)
    }
    /// Installs it as `<plugins>/<id>`, replacing an installed copy of the same id
    /// (wherever that one's directory is), without asking. Nothing is left half-written: a
    /// bad file or a plugin that does not check out leaves the old copy as it was.
    pub fn install_into(&self, plugins: &Path) -> Result<PathBuf, String> {
        self.prepare(plugins)?.commit()
    }
    /// Installs it into this process's plugins directory (after `plugin::install`).
    pub fn install(&self) -> Result<PathBuf, String> {
        let config = plugin::config().ok_or(t!("找不到配置目录"))?;
        self.install_into(&plugin::dir(config))
    }
}

#[cfg(test)]
#[path = "market_install_tests.rs"]
mod install_tests;
#[cfg(test)]
#[path = "market_tests.rs"]
mod tests;
