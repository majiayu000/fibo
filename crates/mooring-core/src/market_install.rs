//! What installing does beyond copying: what a package is checked for and said to do, how it
//! is swapped in, and what is kept of where it came from (see `market`).
use super::*;

/// A package fetched and checked, waiting to be swapped in. Dropping it discards it.
#[derive(Debug)]
pub struct Prepared {
    pub(super) stage: Option<PathBuf>,
    pub(super) plugins: PathBuf,
    pub(super) source: InstalledFrom,
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
        let _ = mooring_platform::fs::create_private_dir(&data);
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
pub(super) const SOURCE_FILE: &str = "source.json";
/// This repository's catalog, whichever way it arrived: shipped with the app or fetched.
pub(super) const OFFICIAL: &str = "官方目录";

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
pub(super) fn replacing(
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
        // Moving between the official keys is the same publisher.
        let still_official = offer.publisher == Publisher::Official
            && r.key.as_deref().is_some_and(|k| OFFICIAL_KEYS.contains(&k));
        match (&r.key, offer.publisher.key()) {
            // The serious one, whatever the address says.
            (Some(was), now) if now.as_ref() != Some(was) && !still_official => out.push(format!(
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
        pid.parse::<i32>()
            .is_ok_and(|p| p != std::process::id() as i32 && mooring_platform::process::exists(p))
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
pub(super) fn loadable(p: &Plugin) -> Result<(), String> {
    let home = mooring_platform::dirs::home().unwrap_or_default();
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
pub(super) fn no_conflict(plugins: &Path, p: &Plugin) -> Result<(), String> {
    let home = mooring_platform::dirs::home().unwrap_or_default();
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
    let home = mooring_platform::dirs::home().unwrap_or_default();
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
            Kind::Rules | Kind::Sidebar | Kind::Effects | Kind::Themes => {}
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
pub(super) fn added(old: &Plugin, new: &Plugin) -> Vec<String> {
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
