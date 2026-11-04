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
//! in (`OFFICIAL_KEY`) and its catalog must be signed by it; another catalog's key is shown
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

/// The index format's major version.
pub const API: u32 = 1;
/// This repository's plugins, as published on GitHub.
pub const DEFAULT_REMOTE: &str =
    "https://raw.githubusercontent.com/majiayu000/mooring/main/plugins/index.json";
/// No plugin file is this large; a bigger one is refused, not written.
const MAX_FILE: u64 = 16 << 20;
/// The public half of the key this repository's catalog is signed with; the private half is
/// `publisher.key` on the maintainer's machine, never in the repository.
pub const OFFICIAL_KEY: &str = "e88de1d56240c9dbf6f150dfdc58347a967d78ed88dc07218cb116c976aa8988";

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

/// Who a catalog's signature says published it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Publisher {
    /// This repository: signed by `OFFICIAL_KEY`, or shipped inside the app itself.
    Official,
    /// Another key, valid for this index.
    Signed { name: String, key: String },
    /// No `index.sig`.
    Unsigned,
}
impl Publisher {
    /// The key to remember with what was installed.
    fn key(&self) -> Option<String> {
        match self {
            Self::Official => Some(OFFICIAL_KEY.to_owned()),
            Self::Signed { key, .. } => Some(key.clone()),
            Self::Unsigned => None,
        }
    }
    /// A key's first characters: enough to see that it changed.
    fn fingerprint(key: &str) -> String {
        key.chars().take(16).collect()
    }
    /// In words, for a confirmation.
    pub fn said(&self) -> String {
        match self {
            Self::Official => OFFICIAL.to_owned(),
            Self::Signed { name, key } => {
                format!("{name}（密钥 {}）", Self::fingerprint(key))
            }
            Self::Unsigned => "没有签名的目录".to_owned(),
        }
    }
}

/// `index.sig`: who signed the bytes of `index.json`, and the signature.
#[derive(Debug, Serialize, Deserialize)]
struct Signature {
    alg: String,
    publisher: String,
    key: String,
    sig: String,
}
/// `publisher.key`: a publisher's private key. Never published.
#[derive(Serialize, Deserialize)]
struct SecretKey {
    publisher: String,
    secret: String,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn from_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// A new publisher key: the file to keep private, and the public key to show.
pub fn new_key(publisher: &str) -> Result<(String, String), String> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| format!("拿不到随机数：{e}"))?;
    let public = ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key();
    let file = SecretKey {
        publisher: publisher.to_owned(),
        secret: to_hex(&secret),
    };
    Ok((
        serde_json::to_string_pretty(&file).map_err(|e| e.to_string())? + "\n",
        to_hex(public.as_bytes()),
    ))
}

/// `index.sig` for these exact `index.json` bytes, from a `publisher.key`.
pub fn sign_index(index: &[u8], secret_key: &[u8]) -> Result<String, String> {
    use ed25519_dalek::Signer;
    let file: SecretKey =
        serde_json::from_slice(secret_key).map_err(|e| format!("私钥文件格式不对：{e}"))?;
    let secret = from_hex::<32>(&file.secret).ok_or("私钥文件格式不对")?;
    let key = ed25519_dalek::SigningKey::from_bytes(&secret);
    let signature = Signature {
        alg: "ed25519".into(),
        publisher: file.publisher,
        key: to_hex(key.verifying_key().as_bytes()),
        sig: to_hex(&key.sign(index).to_bytes()),
    };
    Ok(serde_json::to_string_pretty(&signature).map_err(|e| e.to_string())? + "\n")
}

/// Who signed these index bytes; an invalid signature is an error, never "unsigned".
pub fn verify_index(index: &[u8], signature: &[u8]) -> Result<Publisher, String> {
    use ed25519_dalek::Verifier;
    let s: Signature =
        serde_json::from_slice(signature).map_err(|e| format!("index.sig 格式不对：{e}"))?;
    if s.alg != "ed25519" {
        return Err(format!("index.sig 用的是 {}，这个版本只认 ed25519", s.alg));
    }
    let key = from_hex::<32>(&s.key)
        .and_then(|k| ed25519_dalek::VerifyingKey::from_bytes(&k).ok())
        .ok_or("index.sig 的密钥格式不对")?;
    let sig = from_hex::<64>(&s.sig).ok_or("index.sig 的签名格式不对")?;
    key.verify(index, &ed25519_dalek::Signature::from_bytes(&sig))
        .map_err(|_| "index.json 和它的签名对不上：可能被改过，没有使用".to_owned())?;
    Ok(if s.key == OFFICIAL_KEY {
        Publisher::Official
    } else {
        Publisher::Signed {
            name: s.publisher,
            key: s.key,
        }
    })
}

/// What a catalog at `url` counts as, given its index and signature if it has one. This
/// repository's own address accepts nothing but this repository's key.
fn publisher_of(url: &str, index: &[u8], signature: Option<&[u8]>) -> Result<Publisher, String> {
    let publisher = match signature {
        Some(s) => verify_index(index, s)?,
        None => Publisher::Unsigned,
    };
    if url == DEFAULT_REMOTE && publisher != Publisher::Official {
        return Err("官方目录没有官方的签名，没有使用".to_owned());
    }
    Ok(publisher)
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
            Kind::Actions => "右键菜单或 ⌘P 里点了它，就在终端里运行它写的命令",
            Kind::Hooks => "会话等你、出错、做完或命令结束时，在后台运行它写的命令或发 webhook",
            Kind::Decorations => "定时在窗格所在目录里运行它写的命令，显示第一行输出",
            Kind::Agent => "读这个 Agent 的对话记录，并能用它写的命令启动和恢复会话",
            Kind::Rules => "只决定哪些命令算跑过验证，不运行任何东西",
            Kind::Sidebar => "只决定左栏怎么排；WASM 排法在沙盒里跑，读不到文件和网络",
            Kind::Effects => "只挑选标记完成时播哪种动画，不运行任何东西",
            Kind::Setup => "装上、更新或打开时运行它的 setup 命令，停用或卸载时运行 teardown 撤回，可以改别的程序的设置",
            Kind::Quota => "读它指定的文件或定时运行它写的命令，取某个 Agent 的额度",
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
        out.push(format!("会改别的程序的文件：{}", touches.join("、")));
    }
    match network {
        Some(true) => out.push("会联网".into()),
        Some(false) if runs_commands(provides) => out.push("声明不联网".into()),
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
            return Err(format!("id \"{}\" 不合法", self.id));
        }
        if let Some(level) = self.mooring.filter(|l| *l > plugin::LEVEL) {
            return Err(format!(
                "要更新 Mooring 才能装 {}：它需要插件级别 {level}，这个版本是 {}",
                self.id,
                plugin::LEVEL
            ));
        }
        if let Some(dir) = self.dir.as_deref().filter(|d| !safe_path(d)) {
            return Err(format!("{} 的目录 \"{dir}\" 不安全", self.id));
        }
        if !self.files.iter().any(|f| f.path == "plugin.json") {
            return Err(format!("{} 的文件里没有 plugin.json", self.id));
        }
        for f in &self.files {
            if !safe_path(&f.path) {
                return Err(format!("{} 的文件路径 \"{}\" 不安全", self.id, f.path));
            }
            if f.size > MAX_FILE {
                return Err(format!("{} 的 {} 太大（{} 字节）", self.id, f.path, f.size));
            }
            if f.sha256.len() != 64 || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("{} 的 {} 没有正确的 sha256", self.id, f.path));
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
            .map_err(|e| format!("{}：{e}", found.dir.display()))?;
        if found.dir.file_name() != Some(std::ffi::OsStr::new(&p.id)) {
            return Err(format!(
                "{}：目录名要和 id「{}」一样，安装时按 id 找文件",
                found.dir.display(),
                p.id
            ));
        }
        let mut files = Vec::new();
        for file in shipped_files(&p) {
            let bytes =
                std::fs::read(&file).map_err(|e| format!("读不了 {}：{e}", file.display()))?;
            let path = file
                .strip_prefix(&p.dir)
                .map_err(|_| format!("{} 不在插件目录里", file.display()))?
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
        let p = plugin::check(&e.path()).map_err(|why| format!("{}：{why}", e.path().display()))?;
        if p.id != id {
            return Err(format!("{}：id 要和插件一样（{id}）", e.path().display()));
        }
        let mut files = Vec::new();
        for file in shipped_files(&p) {
            let bytes =
                std::fs::read(&file).map_err(|err| format!("读不了 {}：{err}", file.display()))?;
            files.push(FileEntry {
                path: file
                    .strip_prefix(&p.dir)
                    .map_err(|_| format!("{} 不在插件目录里", file.display()))?
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
        serde_json::from_slice(bytes).map_err(|e| format!("index.json 格式不对：{e}"))?;
    if index.api != API {
        return Err(format!(
            "插件目录格式是 api {}，这个版本只认 api {API}",
            index.api
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
    let exe = std::env::current_exe().ok()?;
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
    let bytes = std::fs::read(dir.join("index.json"))
        .map_err(|e| format!("读不了 {}：{e}", dir.join("index.json").display()))?;
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
        .map_err(|e| format!("运行 curl 失败：{e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if why.is_empty() {
            format!("下载 {url} 失败（curl 退出码 {:?}）", out.status.code())
        } else {
            format!("下载 {url} 失败：{why}")
        });
    }
    Ok(out.stdout)
}

/// Fetches the remote index. Only `https://` and `file://` URLs are followed.
pub fn fetch_remote(url: &str) -> Result<Vec<Offer>, String> {
    let base = url
        .rsplit_once('/')
        .map(|(b, _)| b.to_owned())
        .ok_or_else(|| format!("{url} 不是一个地址"))?;
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
                std::fs::read(&file).map_err(|e| format!("读不了 {}：{e}", file.display()))
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
            (Publisher::Unsigned, Source::Bundled(_)) => OFFICIAL.to_owned(),
            (publisher, _) => publisher.said(),
        }
    }
    /// Where it comes from, in words.
    pub fn origin(&self) -> &'static str {
        match self.source {
            Source::Bundled(_) => "随 App 附带",
            Source::Remote(_) => "远程目录",
        }
    }
    /// Fetches and checks it into a staging directory without touching what is installed.
    /// A package that says more than its listing, or whose files the loaders refuse, stops
    /// here: an update can never replace a working plugin with one that does not load.
    pub fn prepare(&self, plugins: &Path) -> Result<Prepared, String> {
        let e = &self.entry;
        e.check()?;
        std::fs::create_dir_all(plugins).map_err(|err| format!("建不了插件目录：{err}"))?;
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
                return Err(format!(
                    "{} 的 {} 和目录里登记的不一致（大小或 sha256 不对），没有安装",
                    e.id, f.path
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
            return Err(format!(
                "plugin.json 里的 id 是 {}，目录里登记的是 {}",
                p.id, e.id
            ));
        }
        // What you are shown may never be less than what the package says of itself.
        if let Some(k) = p.provides.iter().find(|k| !e.provides.contains(k)) {
            return Err(format!(
                "{} 的 plugin.json 声明了 {}，目录里没有登记，没有安装",
                e.id,
                k.label()
            ));
        }
        if let Some(t) = p.touches.iter().find(|t| !e.touches.contains(t)) {
            return Err(format!(
                "{} 的 plugin.json 说会改 {t}，目录里没有登记，没有安装",
                e.id
            ));
        }
        if p.network == Some(true) && e.network != Some(true) {
            return Err(format!(
                "{} 的 plugin.json 说会联网，目录里没有登记，没有安装",
                e.id
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
                Publisher::Signed { .. } => Some(format!(
                    "发布者：{}，第一次见到这个密钥，以后换了会问你",
                    self.publisher.said()
                )),
                Publisher::Unsigned => Some("这个目录没有签名，不能确认文件是谁发的".to_owned()),
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
        let config = plugin::config().ok_or("找不到配置目录")?;
        self.install_into(&plugin::dir(config))
    }
}

/// A package fetched and checked, waiting to be swapped in. Dropping it discards it.
#[derive(Debug)]
pub struct Prepared {
    stage: Option<PathBuf>,
    plugins: PathBuf,
    source: InstalledFrom,
    pub id: String,
    /// What it will do once installed, read from the package.
    pub disclosure: Vec<String>,
    /// What it can do that the installed version cannot; empty on a first install.
    pub added: Vec<String>,
    /// What replacing the installed copy would lose or change hands.
    pub replacing: Vec<String>,
    /// Whether to confirm first: it runs commands, or an update adds to what it runs.
    pub must_ask: bool,
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(stage) = self.stage.take() {
            let _ = std::fs::remove_dir_all(stage);
        }
    }
}
impl Prepared {
    /// What to tell someone before they confirm, one line each.
    pub fn lines(&self) -> Vec<String> {
        let what = if self.added.is_empty() {
            &self.disclosure
        } else {
            &self.added
        };
        self.replacing.iter().chain(what).cloned().collect()
    }
    /// Swaps it in as `<plugins>/<id>`, replacing whichever directory holds this id now,
    /// and runs its setup. Only a setup that worked makes it the installed version: when an
    /// update's fails, the old version comes back; when a first install's fails, it stays
    /// but off. Either way what the failed setup managed is undone as far as its teardown
    /// can, and no failure is called installed.
    pub fn commit(mut self) -> Result<PathBuf, String> {
        use crate::setup::Phase;
        let stage = self.stage.take().ok_or("已经装过了")?;
        let (plugins, id) = (self.plugins.clone(), self.id.clone());
        let discard = |err: String| {
            let _ = std::fs::remove_dir_all(&stage);
            err
        };
        let _held = plugin::lock(&plugins).map_err(discard)?;
        let target = plugin::discover(&plugins)
            .into_iter()
            .find(|f| f.result.as_ref().is_ok_and(|p| p.id == id))
            .map(|f| f.dir)
            .filter(|d| d.parent() == Some(plugins.as_path()));
        let target = match target {
            Some(dir) => dir,
            // `<id>` may hold another plugin (an example copied and renamed) or a broken
            // one someone is writing: never delete it to make room.
            None if plugins.join(&id).exists() => {
                return Err(discard(format!(
                    "{} 已被别的东西占用（不是插件 {id}），没有安装；把它移走或改名后再装",
                    plugins.join(&id).display(),
                )));
            }
            None => plugins.join(&id),
        };
        let on = !plugin::turned_off(&plugins, &id);
        let had = target.exists();
        let provides_setup =
            |dir: &Path| plugin::check(dir).is_ok_and(|p| p.provides.contains(&Kind::Setup));
        let old_setup = had && provides_setup(&target);
        // The update drops its setup: this is the last time the old teardown exists.
        if on && old_setup && !provides_setup(&stage) {
            crate::setup::run(&plugins, &target, Phase::Teardown).map_err(|err| {
                discard(format!(
                    "没有更新：新版本不再带安装步骤，要先撤回旧版做的事，但 {err}"
                ))
            })?;
        }
        let old = plugins.join(format!(".replaced-{id}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&old);
        if had {
            std::fs::rename(&target, &old)
                .map_err(|err| discard(format!("换不掉旧的 {}：{err}", target.display())))?;
        }
        if let Err(err) = std::fs::rename(&stage, &target) {
            if had {
                let _ = std::fs::rename(&old, &target);
            }
            return Err(discard(format!("装不进 {}：{err}", target.display())));
        }
        if on && let Err(err) = crate::setup::run(&plugins, &target, Phase::Setup) {
            let _ = crate::setup::run(&plugins, &target, Phase::Teardown);
            if had {
                let _ = std::fs::remove_dir_all(&target);
                std::fs::rename(&old, &target)
                    .map_err(|e| format!("{err}；旧版也没能放回去（{e}）：{}", old.display()))?;
                if old_setup {
                    let _ = crate::setup::run(&plugins, &target, Phase::Setup);
                }
                return Err(format!("没有更新，旧版还在：{err}"));
            }
            plugin::mark_off(&plugins, &id).map_err(|e| format!("{err}；也没能停用它：{e}"))?;
            return Err(format!(
                "装上了但没有启用：{err}；修好后在 设置 › 插件 里打开可以重试"
            ));
        }
        let _ = std::fs::remove_dir_all(&old);
        self.save_source();
        Ok(target)
    }
    /// Keeps where it came from, for the next time something offers to replace it.
    fn save_source(&self) {
        let data = crate::setup::data_dir(&self.plugins, &self.id);
        let _ = crate::plugin_context::create_private_dir(&data);
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.source) {
            let _ = std::fs::write(data.join(SOURCE_FILE), bytes);
        }
    }
}

/// Where an installed plugin came from, kept in its data directory.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledFrom {
    /// `OFFICIAL`, or the catalog's address.
    pub from: String,
    /// The files that package listed; anything else in its directory is somebody's own.
    pub files: Vec<String>,
    /// The key it was signed with: a later version signed by another is asked about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}
const SOURCE_FILE: &str = "source.json";
/// This repository's catalog, whichever way it arrived: shipped with the app or fetched.
const OFFICIAL: &str = "官方目录";

/// Every file under `dir`, relative to it; hidden ones are nobody's plugin file.
fn files_under(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(at).into_iter().flatten().flatten() {
            let path = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// What replacing `old` with this package would lose or change hands: it came from somewhere
/// else, it is somebody's own under the same id, or its directory has files no package put
/// there. Each is a reason to ask, whatever the package does.
fn replacing(
    plugins: &Path,
    old: &Plugin,
    new: &Plugin,
    entry: &Entry,
    offer: &Offer,
) -> Vec<String> {
    let from = offer.origin_key();
    let recorded: Option<InstalledFrom> =
        std::fs::read(crate::setup::data_dir(plugins, &old.id).join(SOURCE_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
    let mut out = Vec::new();
    if let Some(r) = &recorded {
        match (&r.key, offer.publisher.key()) {
            // The serious one, whatever the address says.
            (Some(was), now) if now.as_ref() != Some(was) => out.push(format!(
                "发布者变了：原来的密钥是 {}，这次{}",
                Publisher::fingerprint(was),
                now.map_or("没有签名".to_owned(), |k| format!(
                    "是 {}",
                    Publisher::fingerprint(&k)
                ))
            )),
            _ if r.from != from => {
                out.push(format!("来源变了：原来来自 {}，这次来自 {from}", r.from));
            }
            _ => {}
        }
    }
    if old.name != new.name {
        out.push(format!(
            "这是另一个名字的插件：「{}」换成「{}」",
            old.name, new.name
        ));
    }
    let keeps = |f: &String| {
        entry.files.iter().any(|e| &e.path == f)
            || recorded.as_ref().is_some_and(|r| r.files.contains(f))
    };
    let own: Vec<String> = files_under(&old.dir)
        .into_iter()
        .filter(|f| !keeps(f))
        .collect();
    if !own.is_empty() {
        out.push(format!(
            "目录里这些文件不是包里的，换掉后就没了：{}",
            own.join("、")
        ));
    }
    out
}

/// Clears what an install interrupted by a crash left: a staged copy is dropped, and a
/// replaced copy whose plugin never came back is put back.
pub fn recover(plugins: &Path) {
    let alive = |pid: &str| {
        pid.parse::<i32>().is_ok_and(|p| {
            // SAFETY: signal 0 only asks whether the process exists.
            p != std::process::id() as i32 && unsafe { libc::kill(p, 0) } == 0
        })
    };
    let Ok(_held) = plugin::lock(plugins) else {
        return;
    };
    for e in std::fs::read_dir(plugins).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(rest) = name.strip_prefix(".installing-") {
            let mut parts = rest.rsplitn(3, '-');
            let (_n, pid) = (parts.next(), parts.next());
            if !pid.is_some_and(alive) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        } else if let Some(rest) = name.strip_prefix(".replaced-")
            && let Some((id, pid)) = rest.rsplit_once('-')
            && !alive(pid)
        {
            let back = plugins.join(id);
            if back.exists() {
                let _ = std::fs::remove_dir_all(e.path());
            } else {
                let _ = std::fs::rename(e.path(), back);
            }
        }
    }
}

/// Every file of the kinds it provides through the loader that will read it.
fn loadable(p: &Plugin) -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    for kind in &p.provides {
        for loaded in plugin::load_kind(*kind, kind.files(&p.dir), &home) {
            if let Err(why) = loaded.result {
                let file = loaded
                    .file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                return Err(format!("{} 的 {file} 加载不了，没有安装：{why}", p.id));
            }
        }
    }
    // Loading skips an event a newer Mooring may know, so an installed plugin keeps working.
    // A hook left with no event never fires: installing that is a mistake, whichever it is.
    for h in crate::hook::load(p.files(Kind::Hooks)).0 {
        if h.on.is_empty() {
            return Err(format!(
                "{} 的 hook {} 没有这个版本认识的事件（{}），没有安装",
                p.id,
                h.id,
                h.skipped.join("、")
            ));
        }
    }
    Ok(())
}

/// Actions and sidebar presets are named from elsewhere (a switch's file, the choice of
/// sidebar), so their ids stay global. A package reusing one is refused here, rather than
/// breaking whichever plugin happens to load second.
fn no_conflict(plugins: &Path, p: &Plugin) -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    for kind in [Kind::Actions, Kind::Sidebar] {
        if !p.provides.contains(&kind) {
            continue;
        }
        let mut files: Vec<PathBuf> = plugin::discover(plugins)
            .into_iter()
            .filter_map(|f| f.result.ok())
            .filter(|o| o.id != p.id)
            .flat_map(|o| o.files(kind))
            .collect();
        let own = kind.files(&p.dir);
        files.extend(own.iter().cloned());
        for loaded in plugin::load_kind(kind, files, &home) {
            if own.contains(&loaded.file)
                && let Err(why) = loaded.result
            {
                return Err(format!("{} 和已装的插件冲突，没有安装：{why}", p.id));
            }
        }
    }
    Ok(())
}

/// The commands and webhook addresses a plugin declares, as written.
fn declared(p: &Plugin) -> (Vec<String>, Vec<String>) {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let (mut commands, mut webhooks) = (Vec::new(), Vec::new());
    for kind in &p.provides {
        let files = kind.files(&p.dir);
        match kind {
            Kind::Actions => {
                commands.extend(crate::action::load(files).0.into_iter().map(|a| a.command));
            }
            Kind::Hooks => {
                for h in crate::hook::load(files).0 {
                    commands.extend(h.command);
                    webhooks.extend(h.webhook);
                }
            }
            Kind::Decorations => {
                commands.extend(
                    crate::decoration::load(files)
                        .0
                        .into_iter()
                        .map(|d| d.command),
                );
            }
            Kind::Quota => {
                commands.extend(
                    crate::quota::load(files)
                        .0
                        .into_iter()
                        .filter_map(|s| s.command),
                );
            }
            Kind::Setup => {
                for f in files {
                    if let Ok(s) = crate::setup::read(&f) {
                        commands.extend([s.setup, s.teardown]);
                    }
                }
            }
            Kind::Agent => {
                let specs = crate::agent::load_files(files, &home).0;
                for s in specs.into_iter().filter(|s| s.file.is_some()) {
                    commands.extend([s.start, s.resume]);
                }
            }
            Kind::Rules | Kind::Sidebar | Kind::Effects => {}
        }
    }
    (commands, webhooks)
}

/// How many declared commands a confirmation lists before saying how many more.
const SHOWN_COMMANDS: usize = 12;

/// What installing this package means, from the package: the kinds' words, then the files
/// it says it changes, whether it goes online, where webhooks post, and the commands
/// themselves. A webhook is going online whatever the manifest says.
pub fn disclosure_of(p: &Plugin) -> Vec<String> {
    let (commands, webhooks) = declared(p);
    let network = if webhooks.is_empty() {
        p.network
    } else {
        Some(true)
    };
    let mut out = disclosure(&p.provides, &p.touches, network);
    for url in &webhooks {
        out.push(format!("把会话标题和回复第一行发到：{url}"));
    }
    for c in commands.iter().take(SHOWN_COMMANDS) {
        out.push(format!("要跑的命令：{}", crate::content::short(c, 160)));
    }
    if commands.len() > SHOWN_COMMANDS {
        out.push(format!("还有 {} 条命令", commands.len() - SHOWN_COMMANDS));
    }
    out
}

/// What `new` can do that `old` cannot: what an update has to ask about.
fn added(old: &Plugin, new: &Plugin) -> Vec<String> {
    let (old_commands, old_hooks) = declared(old);
    let (new_commands, new_hooks) = declared(new);
    let mut out = Vec::new();
    for k in new.provides.iter().filter(|k| !old.provides.contains(k)) {
        if runs_commands(&[*k]) {
            out.push(format!(
                "这次更新新增了「{}」：{}",
                k.label(),
                permissions(&[*k])[0]
            ));
        }
    }
    for t in new.touches.iter().filter(|t| !old.touches.contains(t)) {
        out.push(format!("这次更新新增了要改的文件：{t}"));
    }
    let online = |p: &Plugin, hooks: &[String]| p.network == Some(true) || !hooks.is_empty();
    if online(new, &new_hooks) && !online(old, &old_hooks) {
        out.push("这次更新开始联网".to_owned());
    }
    for url in new_hooks.iter().filter(|u| !old_hooks.contains(u)) {
        out.push(format!("这次更新新增了发送地址：{url}"));
    }
    for c in new_commands.iter().filter(|c| !old_commands.contains(c)) {
        out.push(format!(
            "这次更新新增或改了命令：{}",
            crate::content::short(c, 160)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kl-market-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    /// What is in the plugins directory but the lock.
    fn entries(plugins: &Path) -> usize {
        std::fs::read_dir(plugins)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name() != ".lock")
            .count()
    }
    fn plugin(root: &Path, id: &str, version: &str) {
        let d = root.join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("plugin.json"),
            format!(
                r#"{{"api":1,"id":"{id}","name":"{id}","version":"{version}","provides":["effects"]}}"#
            ),
        )
        .unwrap();
        std::fs::write(
            d.join("effects.json"),
            r#"{"api":1,"complete":{"style":"sweep","duration_ms":1200}}"#,
        )
        .unwrap();
        // A build leftover that must not travel.
        std::fs::write(d.join("build.sh"), "echo hi").unwrap();
    }

    #[test]
    fn the_shipped_index_is_current() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let want = render(&index_of(&dir).unwrap());
        let have = std::fs::read_to_string(dir.join("index.json")).unwrap_or_default();
        assert!(
            want == have,
            "plugins/index.json 过期了：运行 cargo run -p mooring-host -- plugins index plugins"
        );
        let signature = std::fs::read(dir.join("index.sig")).unwrap_or_default();
        assert_eq!(
            verify_index(have.as_bytes(), &signature),
            Ok(Publisher::Official),
            "plugins/index.sig 过期了：在有官方私钥的机器上运行 mooring plugins index plugins"
        );
    }

    #[test]
    fn installs_only_listed_files_checked_and_replaces_in_place() {
        let catalog = tmp("catalog");
        plugin(&catalog, "quick-sweep", "1.0.0");
        let index = index_of(&catalog).unwrap();
        assert_eq!(
            index.plugins[0]
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["effects.json", "plugin.json"]
        );
        std::fs::write(catalog.join("index.json"), render(&index)).unwrap();
        let offer = Offer {
            entry: index.plugins[0].clone(),
            source: Source::Bundled(catalog.clone()),
            publisher: Publisher::Official,
        };
        let plugins = tmp("plugins");
        // A hand-placed copy under another directory name is replaced where it is.
        plugin(&plugins, "quick-sweep", "0.9.0");
        std::fs::rename(plugins.join("quick-sweep"), plugins.join("mine")).unwrap();
        let at = offer.install_into(&plugins).unwrap();
        assert_eq!(at, plugins.join("mine"));
        assert!(!at.join("build.sh").exists());
        assert!(plugin::check(&at).unwrap().version.as_deref() == Some("1.0.0"));
        assert_eq!(entries(&plugins), 1, "no staging left behind");

        // A tampered file is refused and the installed copy stays.
        std::fs::write(catalog.join("quick-sweep/effects.json"), "{}").unwrap();
        let err = offer.install_into(&plugins).unwrap_err();
        assert!(err.contains("不一致"), "{err}");
        assert!(plugin::check(&at).is_ok());
        assert_eq!(entries(&plugins), 1);
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
    }

    #[test]
    fn unsafe_entries_are_refused_before_fetching() {
        let file = |path: &str| FileEntry {
            path: path.into(),
            sha256: "0".repeat(64),
            size: 1,
        };
        let entry = |id: &str, files: Vec<FileEntry>| Entry {
            id: id.into(),
            name: "x".into(),
            version: "1".into(),
            description: None,
            provides: vec![Kind::Rules],
            mooring: None,
            touches: vec![],
            network: None,
            files,
            dir: None,
            versions: vec![],
        };
        assert!(
            entry("ok", vec![file("plugin.json"), file("a/b.json")])
                .check()
                .is_ok()
        );
        for bad in ["../x", "/etc/passwd", "a/../../x", ".hidden", "a\\b", ""] {
            assert!(
                entry("ok", vec![file("plugin.json"), file(bad)])
                    .check()
                    .is_err(),
                "{bad}"
            );
        }
        assert!(entry("../ok", vec![file("plugin.json")]).check().is_err());
        assert!(entry("ok", vec![file("rules.json")]).check().is_err());
    }

    #[test]
    fn versions_order_as_semver_does() {
        assert!(newer("1.10.0", "1.9.2"));
        assert!(newer("v2", "1.99.99"));
        assert!(newer("1.0.1", "1.0"));
        assert!(!newer("1.0", "1.0.0"));
        assert!(
            !newer("1.0.0-beta", "1.0.0"),
            "a pre-release is older than its release"
        );
        assert!(newer("1.0.0", "1.0.0-rc.1"));
        assert!(newer("1.0.0-beta.11", "1.0.0-beta.2"));
        assert!(newer("1.0.0-rc.1", "1.0.0-beta"));
        assert!(newer("1.0.0-alpha.1", "1.0.0-alpha"));
        assert!(
            !newer("1.0.0+build.7", "1.0.0"),
            "build metadata does not count"
        );
        assert!(!newer("1.0.0", "1.0.0"));
    }

    #[test]
    fn remote_offers_replace_bundled_only_when_newer() {
        let offer = |id: &str, v: &str, remote: bool| Offer {
            entry: Entry {
                id: id.into(),
                name: id.into(),
                version: v.into(),
                description: None,
                provides: vec![Kind::Rules],
                mooring: None,
                touches: vec![],
                network: None,
                files: vec![],
                dir: None,
                versions: vec![],
            },
            source: if remote {
                Source::Remote("https://x".into())
            } else {
                Source::Bundled(PathBuf::new())
            },
            publisher: Publisher::Official,
        };
        let merged = merge(
            vec![offer("a", "0.2.0", false), offer("b", "0.1.0", false)],
            vec![
                offer("a", "0.10.0", true),
                offer("b", "0.1.0", true),
                offer("c", "1", true),
            ],
        );
        let got: Vec<_> = merged
            .iter()
            .map(|o| (o.entry.id.as_str(), o.origin()))
            .collect();
        assert_eq!(
            got,
            [("a", "远程目录"), ("b", "随 App 附带"), ("c", "远程目录")]
        );
        assert!(newer("1.10.0", "1.9.2") && !newer("1.0.0", "1.0.0"));
        assert!(runs_commands(&[Kind::Effects, Kind::Hooks]));
        assert!(!runs_commands(&[Kind::Effects, Kind::Sidebar]));
    }

    #[test]
    fn a_file_url_index_installs_like_a_remote_one() {
        let catalog = tmp("remote");
        plugin(&catalog, "quick-sweep", "2.0.0");
        std::fs::write(
            catalog.join("index.json"),
            render(&index_of(&catalog).unwrap()),
        )
        .unwrap();
        let url = format!("file://{}/index.json", catalog.display());
        let offers = fetch_remote(&url).unwrap();
        assert_eq!(offers.len(), 1);
        let plugins = tmp("remote-plugins");
        let at = offers[0].install_into(&plugins).unwrap();
        assert_eq!(at, plugins.join("quick-sweep"));
        assert!(fetch_remote(&format!("file://{}/missing.json", catalog.display())).is_err());
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
    }

    #[test]
    fn an_index_from_a_newer_mooring_still_lists_what_this_one_can_install() {
        let index = parse(
            br#"{"api":1,"signed":"x","plugins":[
              {"id":"a","name":"A","version":"1.0.0","provides":["actions","canvas"],"homepage":"h",
               "files":[{"path":"plugin.json","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1,"mode":"x"}]},
              {"id":"b","name":"B","version":"1.0.0","provides":["quota"],"mooring":99,
               "files":[{"path":"plugin.json","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1}]}]}"#,
        )
        .unwrap();
        assert_eq!(index.plugins[0].provides, [Kind::Actions]);
        assert!(index.plugins[0].check().is_ok());
        assert!(
            index.plugins[1]
                .check()
                .unwrap_err()
                .contains("更新 Mooring")
        );
    }

    #[test]
    fn the_confirmation_says_what_it_changes_and_whether_it_goes_online() {
        let d = disclosure(
            &[Kind::Setup],
            &["~/.claude/settings.json".into()],
            Some(false),
        );
        assert!(
            d.iter().any(|l| l.contains("~/.claude/settings.json")),
            "{d:?}"
        );
        assert_eq!(d.last().unwrap(), "声明不联网");
        assert!(disclosure(&[Kind::Quota], &[], Some(true)).contains(&"会联网".to_owned()));
        // Nothing runs: nothing to say about the network.
        assert_eq!(disclosure(&[Kind::Effects], &[], Some(false)).len(), 1);
    }

    /// The fallback `<plugins>/<id>` directory may hold a different plugin (a copied
    /// example whose id was changed): installing refuses instead of deleting it.
    #[test]
    fn install_keeps_another_plugin_living_in_the_id_directory() {
        let catalog = tmp("other-catalog");
        plugin(&catalog, "quick-sweep", "1.0.0");
        let offer = Offer {
            entry: index_of(&catalog).unwrap().plugins[0].clone(),
            source: Source::Bundled(catalog.clone()),
            publisher: Publisher::Official,
        };
        let plugins = tmp("other-plugins");
        plugin(&plugins, "other", "0.1.0");
        std::fs::rename(plugins.join("other"), plugins.join("quick-sweep")).unwrap();
        let err = offer.install_into(&plugins).unwrap_err();
        let kept = plugin::check(&plugins.join("quick-sweep")).map(|p| p.id);
        let entries = entries(&plugins);
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(err.contains("占用"), "{err}");
        assert_eq!(kept.as_deref(), Ok("other"));
        assert_eq!(entries, 1, "no staging left behind");
    }

    /// The confirmation for command-running plugins reads the listing's `provides`;
    /// a package whose plugin.json provides more is refused.
    #[test]
    fn install_refuses_a_package_that_provides_more_than_listed() {
        let catalog = tmp("provides");
        plugin(&catalog, "sneaky", "1.0.0");
        let d = catalog.join("sneaky");
        std::fs::write(
            d.join("plugin.json"),
            r#"{"api":1,"id":"sneaky","name":"sneaky","version":"1.0.0","provides":["effects","hooks"]}"#,
        )
        .unwrap();
        std::fs::write(d.join("hooks.json"), r#"{"api":1,"hooks":[]}"#).unwrap();
        let mut entry = index_of(&catalog).unwrap().plugins[0].clone();
        entry.provides = vec![Kind::Effects];
        let offer = Offer {
            entry,
            source: Source::Bundled(catalog.clone()),
            publisher: Publisher::Official,
        };
        let plugins = tmp("provides-into");
        let result = offer.install_into(&plugins);
        let installed = plugins.join("sneaky").exists();
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(result.is_err_and(|e| e.contains("没有登记")));
        assert!(!installed);
    }

    /// One catalog plugin of `kinds`, its declaration files as given, ready to offer.
    fn offer_of(catalog: &Path, manifest: &str, files: &[(&str, &str)]) -> Offer {
        let id: serde_json::Value = serde_json::from_str(manifest).unwrap();
        let id = id["id"].as_str().unwrap().to_owned();
        let d = catalog.join(&id);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("plugin.json"), manifest).unwrap();
        for (name, body) in files {
            std::fs::write(d.join(name), body).unwrap();
        }
        let entry = index_of(catalog)
            .unwrap()
            .plugins
            .into_iter()
            .find(|e| e.id == id)
            .unwrap();
        Offer {
            entry,
            source: Source::Bundled(catalog.to_owned()),
            publisher: Publisher::Official,
        }
    }

    /// Hashes all right, contents that do not load: the working version stays.
    #[test]
    fn a_package_that_does_not_load_leaves_the_installed_version() {
        let catalog = tmp("broken-update");
        let plugins = tmp("broken-update-into");
        const GOOD: &str = r#"{"api":1,"hooks":[{"id":"ok","on":["done"],"command":"true"}]}"#;
        offer_of(
            &catalog,
            r#"{"api":1,"id":"h","name":"H","version":"1.0.0","provides":["hooks"]}"#,
            &[("hooks.json", GOOD)],
        )
        .install_into(&plugins)
        .unwrap();
        let err = offer_of(
            &catalog,
            r#"{"api":1,"id":"h","name":"H","version":"2.0.0","provides":["hooks"]}"#,
            &[(
                "hooks.json",
                r#"{"api":1,"hooks":[{"id":"ok","on":["dne"],"command":"true"}]}"#,
            )],
        )
        .install_into(&plugins)
        .unwrap_err();
        let kept = plugin::check(&plugins.join("h")).unwrap().version;
        let hooks = std::fs::read_to_string(plugins.join("h/hooks.json")).unwrap();
        let entries = entries(&plugins);
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(err.contains("dne"), "{err}");
        assert_eq!(kept.as_deref(), Some("1.0.0"));
        assert_eq!(hooks, GOOD);
        assert_eq!(entries, 1, "no staging left behind");
    }

    /// The listing says less than the package about what it changes or whether it goes
    /// online: what you would be shown is not what it is, so it is refused.
    #[test]
    fn a_package_may_not_claim_more_than_its_listing() {
        let catalog = tmp("liar");
        let plugins = tmp("liar-into");
        let mut offer = offer_of(
            &catalog,
            r#"{"api":1,"id":"l","name":"L","version":"1.0.0","provides":["effects"],"network":true,"touches":["~/.ssh/config"]}"#,
            &[("effects.json", r#"{"api":1,"complete":{"style":"sweep"}}"#)],
        );
        let honest = offer.clone();
        offer.entry.touches.clear();
        let touches = offer.prepare(&plugins).unwrap_err();
        offer.entry.touches = honest.entry.touches.clone();
        offer.entry.network = Some(false);
        let network = offer.prepare(&plugins).unwrap_err();
        let ok = honest.prepare(&plugins).map(|p| p.disclosure.clone());
        let entries = entries(&plugins);
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(touches.contains("~/.ssh/config"), "{touches}");
        assert!(network.contains("联网"), "{network}");
        assert!(ok.unwrap().iter().any(|l| l.contains("~/.ssh/config")));
        assert_eq!(entries, 0, "a dropped preparation leaves nothing");
    }

    /// The confirmation reads the package: webhook addresses and commands by name, and a
    /// webhook counts as going online whatever the manifest says.
    #[test]
    fn the_confirmation_names_commands_and_webhooks() {
        let catalog = tmp("names");
        let plugins = tmp("names-into");
        let prepared = offer_of(
            &catalog,
            r#"{"api":1,"id":"n","name":"N","version":"1.0.0","provides":["hooks"]}"#,
            &[(
                "hooks.json",
                r#"{"api":1,"hooks":[{"id":"a","on":["done"],"webhook":"https://example.invalid/x"},{"id":"b","on":["done"],"command":"say hi"}]}"#,
            )],
        )
        .prepare(&plugins)
        .unwrap();
        let (lines, ask) = (prepared.lines(), prepared.must_ask);
        drop(prepared);
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(ask);
        assert!(lines.contains(&"会联网".to_owned()), "{lines:?}");
        assert!(
            lines
                .iter()
                .any(|l| l.contains("https://example.invalid/x")),
            "{lines:?}"
        );
        assert!(lines.iter().any(|l| l.contains("say hi")), "{lines:?}");
    }

    /// An update is asked about exactly when it adds to what runs; animations are not.
    #[test]
    fn an_update_asks_only_when_it_adds_to_what_runs() {
        let catalog = tmp("adds");
        let plugins = tmp("adds-into");
        const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
        offer_of(
            &catalog,
            r#"{"api":1,"id":"u","name":"U","version":"1.0.0","provides":["effects"]}"#,
            &[("effects.json", EFFECT)],
        )
        .install_into(&plugins)
        .unwrap();
        let same = offer_of(
            &catalog,
            r#"{"api":1,"id":"u","name":"U","version":"1.1.0","provides":["effects"]}"#,
            &[("effects.json", r#"{"api":1,"complete":{"style":"fade"}}"#)],
        )
        .prepare(&plugins)
        .unwrap();
        let (same_asks, same_added) = (same.must_ask, same.added.clone());
        drop(same);
        let more = offer_of(
            &catalog,
            r#"{"api":1,"id":"u","name":"U","version":"2.0.0","provides":["effects","setup"]}"#,
            &[
                ("effects.json", EFFECT),
                (
                    "setup.json",
                    r#"{"api":1,"setup":"evil on","teardown":"evil off"}"#,
                ),
            ],
        )
        .prepare(&plugins)
        .unwrap();
        let (more_asks, more_added) = (more.must_ask, more.added.clone());
        drop(more);
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(!same_asks, "{same_added:?}");
        assert!(more_asks);
        assert!(
            more_added.iter().any(|l| l.contains("安装步骤")),
            "{more_added:?}"
        );
        assert!(
            more_added.iter().any(|l| l.contains("evil on")),
            "{more_added:?}"
        );
    }

    /// An update whose setup fails puts the old version back; a first install's stays off.
    /// Both undo what the failed setup did, and neither counts as installed.
    #[test]
    fn a_setup_that_fails_never_counts_as_installed() {
        let catalog = tmp("setup-fails");
        let plugins = tmp("setup-fails-into");
        let done = catalog.join("undone");
        const ONE: &str = r#"{"api":1,"id":"f","name":"F","version":"1.0.0","provides":["setup"]}"#;
        const TWO: &str = r#"{"api":1,"id":"f","name":"F","version":"2.0.0","provides":["setup"]}"#;
        let fails = format!(
            r#"{{"api":1,"setup":"exit 9","teardown":"touch {}"}}"#,
            done.display()
        );
        let works = r#"{"api":1,"setup":"true","teardown":"true"}"#;
        // First install, failing.
        let err = offer_of(&catalog, ONE, &[("setup.json", &fails)])
            .install_into(&plugins)
            .unwrap_err();
        assert!(
            err.contains("没有启用") && err.contains("退出码 9"),
            "{err}"
        );
        assert!(plugin::turned_off(&plugins, "f"));
        assert!(done.exists(), "its teardown ran");
        // Fixed and turned on; then an update whose setup fails.
        offer_of(&catalog, ONE, &[("setup.json", works)])
            .install_into(&plugins)
            .unwrap();
        plugin::set_enabled_in(&plugins, "f", true, false).unwrap();
        let err = offer_of(&catalog, TWO, &[("setup.json", &fails)])
            .install_into(&plugins)
            .unwrap_err();
        assert!(err.contains("旧版还在"), "{err}");
        let kept = plugin::check(&plugins.join("f")).unwrap();
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert_eq!(kept.version.as_deref(), Some("1.0.0"));
    }

    /// A version that drops its setup: the old teardown runs before its files are gone, and
    /// if it cannot, the update does not happen.
    #[test]
    fn dropping_setup_in_an_update_undoes_it_first() {
        let catalog = tmp("drops-setup");
        let plugins = tmp("drops-setup-into");
        let wrapped = catalog.join("wrapped");
        const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
        let v1 = |teardown: String| {
            format!(
                r#"{{"api":1,"setup":"touch {}","teardown":"{teardown}"}}"#,
                wrapped.display()
            )
        };
        const ONE: &str =
            r#"{"api":1,"id":"w","name":"W","version":"1.0.0","provides":["setup","effects"]}"#;
        let two = || {
            offer_of(
                &catalog,
                r#"{"api":1,"id":"w","name":"W","version":"2.0.0","provides":["effects"]}"#,
                &[("effects.json", EFFECT)],
            )
        };
        offer_of(
            &catalog,
            ONE,
            &[
                ("setup.json", &v1("exit 3".into())),
                ("effects.json", EFFECT),
            ],
        )
        .install_into(&plugins)
        .unwrap();
        let err = two().install_into(&plugins).unwrap_err();
        assert!(
            err.contains("没有更新") && err.contains("退出码 3"),
            "{err}"
        );
        assert_eq!(
            plugin::check(&plugins.join("w"))
                .unwrap()
                .version
                .as_deref(),
            Some("1.0.0")
        );
        offer_of(
            &catalog,
            ONE,
            &[
                ("setup.json", &v1(format!("rm {}", wrapped.display()))),
                ("effects.json", EFFECT),
            ],
        )
        .install_into(&plugins)
        .unwrap();
        assert!(wrapped.exists());
        two().install_into(&plugins).unwrap();
        let (gone, version) = (
            !wrapped.exists(),
            plugin::check(&plugins.join("w")).unwrap().version,
        );
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(gone, "the old teardown ran");
        assert_eq!(version.as_deref(), Some("2.0.0"));
    }

    /// Someone's own plugin under the same id, files no package put there, another catalog:
    /// each is asked about even though an animation alone would not be.
    #[test]
    fn replacing_something_of_someone_elses_is_asked_about() {
        let catalog = tmp("replacing");
        let plugins = tmp("replacing-into");
        const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
        // Written by hand, with notes beside it.
        let mine = plugins.join("m");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(
            mine.join("plugin.json"),
            r#"{"api":1,"id":"m","name":"我的","version":"0.1.0","provides":["effects"]}"#,
        )
        .unwrap();
        std::fs::write(mine.join("effects.json"), EFFECT).unwrap();
        std::fs::write(mine.join("NOTES.md"), "notes").unwrap();
        let offer = offer_of(
            &catalog,
            r#"{"api":1,"id":"m","name":"别人的","version":"9.0.0","provides":["effects"]}"#,
            &[("effects.json", EFFECT)],
        );
        let asked = offer.prepare(&plugins).unwrap();
        let lines = asked.lines().join("\n");
        assert!(asked.must_ask);
        assert!(
            lines.contains("NOTES.md") && lines.contains("别人的"),
            "{lines}"
        );
        asked.commit().unwrap();
        // Now recorded as from the official catalog; the same from elsewhere is a change.
        let mut other = offer.clone();
        other.entry.version = "9.1.0".into();
        other.source = Source::Remote("https://other.invalid/p".into());
        other.publisher = Publisher::Unsigned;
        let from = other.replacing_for_test(&plugins);
        let same = offer.prepare(&plugins).unwrap().must_ask;
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(from.iter().any(|l| l.contains("发布者变了")), "{from:?}");
        assert!(
            !same,
            "the same package from the same place needs no question"
        );
    }

    #[test]
    fn what_a_crash_left_is_cleared_and_a_lost_plugin_comes_back() {
        let plugins = tmp("recover");
        // A pid no process has: the highest possible, far above macOS's limit.
        let dead = 99_999_999;
        std::fs::create_dir_all(plugins.join(format!(".installing-a-b-{dead}-0"))).unwrap();
        std::fs::create_dir_all(plugins.join(format!(".replaced-lost-{dead}"))).unwrap();
        std::fs::write(plugins.join(format!(".replaced-lost-{dead}/keep")), "x").unwrap();
        std::fs::create_dir_all(plugins.join(format!(".replaced-here-{dead}"))).unwrap();
        std::fs::create_dir_all(plugins.join("here")).unwrap();
        recover(&plugins);
        let keep = plugins.join("lost/keep").exists();
        let hidden = std::fs::read_dir(&plugins)
            .unwrap()
            .filter(|e| {
                let n = e.as_ref().unwrap().file_name();
                let n = n.to_string_lossy();
                n.starts_with('.') && n != ".lock"
            })
            .count();
        let _ = std::fs::remove_dir_all(&plugins);
        assert!(keep);
        assert_eq!(hidden, 0);
    }

    impl Offer {
        /// What `prepare` would say about replacing, without fetching a remote's files.
        fn replacing_for_test(&self, plugins: &Path) -> Vec<String> {
            let old = plugin::discover(plugins)
                .into_iter()
                .find_map(|f| f.result.ok().filter(|o| o.id == self.entry.id))
                .unwrap();
            replacing(plugins, &old, &old, &self.entry, self)
        }
    }

    /// A signature vouches for these exact bytes and names its key; any change breaks it.
    #[test]
    fn a_catalogs_signature_says_who_and_fails_on_any_change() {
        let (secret, public) = new_key("别人的目录").unwrap();
        let sig = sign_index(b"{\"api\":1}", secret.as_bytes()).unwrap();
        assert_eq!(
            verify_index(b"{\"api\":1}", sig.as_bytes()),
            Ok(Publisher::Signed {
                name: "别人的目录".into(),
                key: public.clone()
            })
        );
        assert!(
            verify_index(b"{\"api\":2}", sig.as_bytes())
                .unwrap_err()
                .contains("改过")
        );
        let (other, _) = new_key("x").unwrap();
        let mut forged: serde_json::Value =
            serde_json::from_str(&sign_index(b"{\"api\":1}", other.as_bytes()).unwrap()).unwrap();
        forged["key"] = public.into();
        assert!(verify_index(b"{\"api\":1}", forged.to_string().as_bytes()).is_err());
        assert!(verify_index(b"x", b"not json").is_err());
    }

    /// This repository's address takes nothing but this repository's key; elsewhere an
    /// unsigned catalog is allowed and a bad signature is not.
    #[test]
    fn the_official_address_accepts_only_the_official_key() {
        let index = b"{}";
        let (secret, _) = new_key("冒充者").unwrap();
        let sig = sign_index(index, secret.as_bytes()).unwrap();
        assert!(publisher_of(DEFAULT_REMOTE, index, None).is_err());
        assert!(publisher_of(DEFAULT_REMOTE, index, Some(sig.as_bytes())).is_err());
        let elsewhere = "https://example.invalid/index.json";
        assert_eq!(
            publisher_of(elsewhere, index, None),
            Ok(Publisher::Unsigned)
        );
        assert!(matches!(
            publisher_of(elsewhere, index, Some(sig.as_bytes())),
            Ok(Publisher::Signed { .. })
        ));
        assert!(publisher_of(elsewhere, b"changed", Some(sig.as_bytes())).is_err());
    }

    /// Installed from one key: the next version signed by another, or not signed at all, is
    /// asked about even for an animation. The same key from a new address is not.
    #[test]
    fn a_change_of_publisher_is_asked_about() {
        let catalog = tmp("publisher");
        let plugins = tmp("publisher-into");
        const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
        let signed = |key: &str| Publisher::Signed {
            name: "作者".into(),
            key: key.into(),
        };
        let (key_a, key_b) = ("a".repeat(64), "b".repeat(64));
        let mut offer = offer_of(
            &catalog,
            r#"{"api":1,"id":"s","name":"S","version":"1.0.0","provides":["effects"]}"#,
            &[("effects.json", EFFECT)],
        );
        offer.publisher = signed(&key_a);
        let first = offer.prepare(&plugins).unwrap();
        assert!(first.must_ask, "a key not seen before");
        assert!(first.lines().join("").contains(&key_a[..16]));
        first.commit().unwrap();
        let again = offer.prepare(&plugins).unwrap();
        assert!(!again.must_ask, "{:?}", again.lines());
        drop(again);
        offer.publisher = signed(&key_b);
        let changed = offer.prepare(&plugins).unwrap();
        assert!(changed.must_ask && changed.lines().join("").contains("发布者变了"));
        drop(changed);
        offer.publisher = Publisher::Unsigned;
        let unsigned = offer.prepare(&plugins).unwrap();
        assert!(unsigned.must_ask && unsigned.lines().join("").contains("没有签名"));
        drop(unsigned);
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
    }

    /// A catalog keeping an older version: a build too old for the newest gets that one, from
    /// its own directory, and a named version can be asked for.
    #[test]
    fn a_build_too_old_for_the_newest_gets_the_newest_it_can_run() {
        let catalog = tmp("versions");
        let plugins = tmp("versions-into");
        const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
        let write_plugin = |dir: &Path, version: &str, level: u32| {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(
                dir.join("plugin.json"),
                format!(
                    r#"{{"api":1,"id":"v","name":"V","version":"{version}","provides":["effects"],"mooring":{level}}}"#
                ),
            )
            .unwrap();
            std::fs::write(dir.join("effects.json"), EFFECT).unwrap();
        };
        write_plugin(&catalog.join("v"), "3.0.0", 1);
        write_plugin(&catalog.join("v/versions/2.0.0"), "2.0.0", 1);
        write_plugin(&catalog.join("v/versions/1.0.0"), "1.0.0", 1);
        let mut entry = index_of(&catalog).unwrap().plugins.remove(0);
        assert_eq!(
            entry
                .versions
                .iter()
                .map(|e| e.version.as_str())
                .collect::<Vec<_>>(),
            ["2.0.0", "1.0.0"]
        );
        assert_eq!(entry.versions[0].dir.as_deref(), Some("versions/2.0.0"));
        // What this build would see if the newest needed a later Mooring.
        entry.mooring = Some(plugin::LEVEL + 1);
        let chosen = entry.for_this_build();
        assert_eq!(chosen.version, "2.0.0");
        assert_eq!(chosen.versions.len(), 2);
        let offer = Offer {
            entry: chosen,
            source: Source::Bundled(catalog.clone()),
            publisher: Publisher::Official,
        };
        offer.install_into(&plugins).unwrap();
        assert_eq!(
            plugin::check(&plugins.join("v"))
                .unwrap()
                .version
                .as_deref(),
            Some("2.0.0")
        );
        assert!(
            !plugins.join("v/versions").exists(),
            "older versions do not travel"
        );
        offer.pick("1.0.0").unwrap().install_into(&plugins).unwrap();
        assert_eq!(
            plugin::check(&plugins.join("v"))
                .unwrap()
                .version
                .as_deref(),
            Some("1.0.0")
        );
        assert!(offer.pick("9.9.9").is_none());
        // A newer remote that cannot run here does not push out a bundled one that can.
        let mut remote = offer.clone();
        remote.entry.version = "9.0.0".into();
        remote.entry.mooring = Some(plugin::LEVEL + 1);
        let merged = merge(vec![offer.clone()], vec![remote]);
        assert_eq!(merged[0].entry.version, "2.0.0");
        // None it can run: the newest stays, for check to say so.
        entry
            .versions
            .iter_mut()
            .for_each(|e| e.mooring = Some(plugin::LEVEL + 1));
        assert_eq!(entry.for_this_build().version, "3.0.0");
        assert!(
            entry
                .for_this_build()
                .check()
                .unwrap_err()
                .contains("更新 Mooring")
        );
        let _ = std::fs::remove_dir_all(&catalog);
        let _ = std::fs::remove_dir_all(&plugins);
    }

    #[test]
    fn a_version_directory_outside_the_plugin_is_refused() {
        let catalog = tmp("dir-escape");
        let mut offer = offer_of(
            &catalog,
            r#"{"api":1,"id":"e","name":"E","version":"1.0.0","provides":["effects"]}"#,
            &[("effects.json", r#"{"api":1,"complete":{"style":"sweep"}}"#)],
        );
        offer.entry.dir = Some("../../etc".into());
        let err = offer.entry.check().unwrap_err();
        let _ = std::fs::remove_dir_all(&catalog);
        assert!(err.contains("不安全"), "{err}");
    }
}
