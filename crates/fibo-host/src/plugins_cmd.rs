//! `fibo plugins`: checks the plugins under the config directory with the same code
//! the app loads them with, so an Agent that wrote one knows before a restart whether it
//! will take effect, and why not; and installs, removes and configures them.
use fibo_core::market;
use fibo_core::plugin::{self, Kind};
use std::io;
use std::path::{Path, PathBuf};

#[path = "plugins_run.rs"]
mod plugins_run;
use plugins_run::usage_text;
#[cfg(test)]
pub use plugins_run::USAGE;
#[cfg(test)]
use plugins_run::{try_run, write_note, MARK, MARK_EN};
pub use plugins_run::{write_agents_md, COMMAND_USAGE};

const KINDS: [Kind; 10] = [
    Kind::Agent,
    Kind::Actions,
    Kind::Rules,
    Kind::Sidebar,
    Kind::Hooks,
    Kind::Decorations,
    Kind::Effects,
    Kind::Setup,
    Kind::Quota,
    Kind::Themes,
];

#[derive(serde::Serialize)]
struct Checked {
    dir: PathBuf,
    id: Option<String>,
    name: Option<String>,
    enabled: bool,
    /// Why the plugin as a whole cannot load.
    error: Option<String>,
    files: Vec<File>,
    /// Files it has but does not declare: they never load.
    unused: Vec<String>,
    /// Kinds and fields this build skipped.
    warnings: Vec<String>,
}

fn ago(secs: u64) -> String {
    match secs {
        s if s < 60 => t!("{n} 秒前", n = s),
        s if s < 3600 => t!("{n} 分前", n = s / 60),
        s if s < 86_400 => t!("{n} 小时前", n = s / 3600),
        s => t!("{n} 天前", n = s / 86_400),
    }
}

#[derive(serde::Serialize)]
struct File {
    kind: &'static str,
    file: PathBuf,
    ok: Option<String>,
    error: Option<String>,
}

pub(super) fn plugins_dir() -> io::Result<PathBuf> {
    Ok(plugin::dir(&fibo_core::agent::config_dir().ok_or_else(
        || io::Error::other(t!("找不到配置目录：HOME 没有设置")),
    )?))
}

/// `<config>/publisher.key`, where a publisher's private key lives by default.
fn default_key() -> io::Result<PathBuf> {
    let config = fibo_core::agent::config_dir()
        .ok_or_else(|| io::Error::other(t!("找不到配置目录：HOME 没有设置")))?;
    Ok(config.join("publisher.key"))
}

/// Writes a file only its owner can read, from the moment it exists.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    fibo_platform::fs::private_file()
        .create_new(true)
        .write(true)
        .open(path)?
        .write_all(bytes)
}

fn offers(remote: bool) -> io::Result<Vec<market::Offer>> {
    let bundled = market::bundled().map_err(io::Error::other)?;
    if !remote {
        return Ok(bundled);
    }
    let fetched = market::fetch_remote(&market::remote_url()).map_err(io::Error::other)?;
    Ok(market::merge(bundled, fetched))
}

fn market_cmd(command: &str, mut args: impl Iterator<Item = String>) -> io::Result<()> {
    let mut rest: Vec<String> = Vec::new();
    let mut remote = false;
    let mut check_only = false;
    let mut yes = false;
    let mut send = false;
    let mut force = false;
    let (mut event, mut cwd, mut key_file, mut publisher) = (None, None, None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--remote" => remote = true,
            "--yes" | "-y" => yes = true,
            "--check" => check_only = true,
            "--send" => send = true,
            "--force" => force = true,
            "--event" => event = args.next(),
            "--cwd" => cwd = args.next(),
            "--key" => key_file = args.next().map(PathBuf::from),
            "--name" => publisher = args.next(),
            _ => rest.push(a),
        }
    }
    match (command, rest.as_slice()) {
        ("keygen", []) => {
            let path = match key_file {
                Some(path) => path,
                None => default_key()?,
            };
            if path.exists() {
                return Err(io::Error::other(t!(
                    "{path} 已经有了，不覆盖：丢了原来的密钥，已经发出去的插件就没法更新了",
                    path = path.display()
                )));
            }
            let (secret, public) =
                market::new_key(publisher.as_deref().unwrap_or_else(|| t!("未命名")))
                    .map_err(io::Error::other)?;
            write_private(&path, secret.as_bytes())?;
            println!(
                "{}",
                t!(
                    "私钥：{path}（保管好，别进仓库）\n公钥：{public}",
                    path = path.display(),
                    public
                )
            );
            Ok(())
        }
        ("index", [dir]) => {
            let dir = Path::new(dir);
            let text = market::render(&market::index_of(dir).map_err(io::Error::other)?);
            let path = dir.join("index.json");
            let sig_path = dir.join("index.sig");
            let same = std::fs::read_to_string(&path).is_ok_and(|txt| txt == text);
            if check_only {
                if !same {
                    eprintln!("{}", t!("{path} 过期了", path = path.display()));
                    std::process::exit(1);
                }
                match std::fs::read(&sig_path) {
                    Ok(sig) => match market::verify_index(text.as_bytes(), &sig) {
                        Ok(who) => {
                            println!(
                                "{}",
                                t!(
                                    "{path} 是最新的，签名有效：{who}",
                                    path = path.display(),
                                    who = who.said()
                                )
                            )
                        }
                        Err(why) => {
                            eprintln!("{}", t!("{path}：{why}", path = sig_path.display(), why));
                            std::process::exit(1);
                        }
                    },
                    Err(_) => {
                        println!("{}", t!("{path} 是最新的，没有签名", path = path.display()))
                    }
                }
                return Ok(());
            }
            if same {
                println!("{}", t!("{path} 是最新的", path = path.display()));
            } else {
                std::fs::write(&path, &text)?;
                println!("{}", t!("写好了 {path}", path = path.display()));
            }
            // Signed whenever the key is here, so publishing cannot forget to.
            let key = match key_file {
                Some(file) => Some(file),
                None => default_key().ok().filter(|f| f.is_file()),
            };
            match key {
                Some(file) => {
                    let sig = market::sign_index(text.as_bytes(), &std::fs::read(&file)?)
                        .map_err(io::Error::other)?;
                    std::fs::write(&sig_path, sig)?;
                    println!("{}", t!("签好了 {path}", path = sig_path.display()));
                }
                None if sig_path.exists() && !same => {
                    eprintln!(
                        "{}",
                        t!(
                            "注意：没有私钥，{path} 已经和新的目录对不上；这是本仓库的 plugins/ 的话别提交这两个文件，推送后 CI 会生成并签名",
                            path = sig_path.display()
                        )
                    );
                }
                None => {}
            }
            Ok(())
        }
        ("available", []) => {
            let installed = plugin::discover(&plugins_dir()?);
            let offered = offers(remote)?;
            if offered.is_empty() {
                println!(
                    "{}",
                    if remote {
                        t!("附带的和远程目录里都没有插件")
                    } else {
                        t!("这个 App 没有附带插件；加 --remote 查远程目录")
                    }
                );
            }
            for o in offered {
                let e = &o.entry;
                let have = installed
                    .iter()
                    .filter_map(|f| f.result.as_ref().ok())
                    .find(|p| p.id == e.id);
                let state = match have {
                    None => String::new(),
                    Some(p)
                        if p.version
                            .as_deref()
                            .is_some_and(|v| market::newer(&e.version, v)) =>
                    {
                        t!(
                            "（已装 v{ver}，可更新）",
                            ver = p.version.as_deref().unwrap_or("?")
                        )
                    }
                    Some(_) => t!("（已安装）").into(),
                };
                println!(
                    "{}\n    {}",
                    t!(
                        "{id} v{ver}「{name}」{state} · {origin}",
                        id = e.id,
                        ver = e.version,
                        name = e.name,
                        state,
                        origin = o.origin()
                    ),
                    e.description.as_deref().unwrap_or("")
                );
            }
            Ok(())
        }
        ("install", [wanted]) => {
            let (id, version) = match wanted.split_once('@') {
                Some((id, version)) => (id, Some(version)),
                None => (wanted.as_str(), None),
            };
            let offer = offers(remote)?
                .into_iter()
                .find(|o| o.entry.id == id)
                .ok_or_else(|| io::Error::other(t!("目录里没有叫 {id} 的插件", id)))?;
            let offer = match version {
                Some(v) => offer
                    .pick(v)
                    .ok_or_else(|| io::Error::other(t!("目录里 {id} 没有 {v} 这个版本", id, v)))?,
                None => offer,
            };
            // Read from the package itself, before anything installed changes.
            let prepared = offer.prepare(&plugins_dir()?).map_err(io::Error::other)?;
            println!(
                "{}",
                t!("{id} v{ver} 装上后会：", id, ver = offer.entry.version)
            );
            for p in prepared.lines() {
                println!("  · {p}");
            }
            // An Agent or a script installing one that runs commands says so on purpose,
            // and what it will do is now in its output for the user to read.
            use std::io::IsTerminal;
            if prepared.must_ask && !yes {
                if !std::io::stdin().is_terminal() {
                    return Err(io::Error::other(t!("先确认上面这些，再加 --yes 安装")));
                }
                eprint!("{}", t!("确认安装？[y/N] "));
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim(), "y" | "Y" | "yes") {
                    return Err(io::Error::other(t!("没有安装")));
                }
            }
            let result = prepared.commit().map_err(io::Error::other);
            // Even a failed one may have changed what is installed.
            let told = tell_running();
            let at = result?;
            println!(
                "{}",
                t!(
                    "装好了 {id} v{ver} → {at}",
                    id,
                    ver = offer.entry.version,
                    at = at.display()
                )
            );
            println!("{told}");
            Ok(())
        }
        ("uninstall", [id]) => {
            let warned =
                plugin::uninstall_in(&plugins_dir()?, id, force).map_err(io::Error::other)?;
            println!("{}", t!("卸载了 {id}", id));
            if let Some(w) = warned {
                println!("{}", t!("注意：{w}", w));
            }
            println!("{}", tell_running());
            Ok(())
        }
        (verb @ ("enable" | "disable"), [id]) => {
            let warned = plugin::set_enabled_in(&plugins_dir()?, id, verb == "enable", force)
                .map_err(io::Error::other)?;
            println!(
                "{}",
                if verb == "enable" {
                    t!("打开了 {id}", id)
                } else {
                    t!("停用了 {id}", id)
                }
            );
            if let Some(w) = warned {
                println!("{}", t!("注意：{w}", w));
            }
            println!("{}", tell_running());
            Ok(())
        }
        ("run", [id, declaration]) => plugins_run::try_run(id, declaration, event, cwd, send),
        ("log", [id]) => {
            let runs = fibo_core::plugin_context::runs(&plugins_dir()?, id);
            if runs.is_empty() {
                println!("{}", t!("{id} 还没有运行记录", id));
            }
            let now = fibo_core::content::now();
            for r in runs {
                println!(
                    "{} {:>10}  {:<28} {:>5} ms  {}",
                    if r.ok { "✓" } else { "✗" },
                    ago(now.saturating_sub(r.at)),
                    r.what,
                    r.ms,
                    r.detail
                );
            }
            Ok(())
        }
        ("set", [id, rest @ ..]) => {
            let dir = plugins_dir()?;
            let p = plugin::discover(&dir)
                .into_iter()
                .find_map(|f| f.result.ok().filter(|p| &p.id == id))
                .ok_or_else(|| io::Error::other(t!("没有装叫 {id} 的插件", id)))?;
            match rest {
                [] => {
                    if p.options.is_empty() {
                        println!("{}", t!("{id} 没有设置项", id));
                    }
                    for (o, v) in fibo_core::plugin_context::values(&dir, &p) {
                        let shown = if o.secret && v == serde_json::Value::String(String::new()) {
                            t!("未设置").to_owned()
                        } else {
                            fibo_core::plugin_context::Opt::text(&v)
                        };
                        let desc = o.description.map(|d| t!("：{d}", d)).unwrap_or_default();
                        println!("{} = {shown}    {}{desc}", o.key, o.title);
                    }
                    Ok(())
                }
                [key, value] => {
                    let opt = p.options.iter().find(|o| &o.key == key).ok_or_else(|| {
                        io::Error::other(t!("{id} 没有叫 {key} 的设置项", id, key))
                    })?;
                    // A command line is in the shell's history and in `ps`: a secret comes on
                    // standard input, and is never printed back.
                    let (v, shown) = if opt.secret {
                        if value != "-" {
                            return Err(io::Error::other(t!(
                                "{key} 是密钥，别写在命令行里：printf %s 值 | fibo plugins set {id} {key} -",
                                key,
                                id
                            )));
                        }
                        let mut text = String::new();
                        io::Read::read_to_string(&mut io::stdin(), &mut text)?;
                        let text = text.trim_end_matches(['\n', '\r']).to_owned();
                        let said = if text.is_empty() {
                            t!("清空了")
                        } else {
                            t!("存进了钥匙串")
                        };
                        (serde_json::Value::String(text), said.to_owned())
                    } else {
                        (
                            opt.parse(value).map_err(io::Error::other)?,
                            t!("改成了 {value}", value),
                        )
                    };
                    fibo_core::plugin_context::set(&dir, id, key, v).map_err(io::Error::other)?;
                    println!(
                        "{}",
                        t!("{id} 的 {key} {shown}，下次运行就用", id, key, shown)
                    );
                    Ok(())
                }
                _ => Err(io::Error::other(usage_text())),
            }
        }
        _ => Err(io::Error::other(usage_text())),
    }
}

/// Tells a running Fibo what changed, so nobody has to go and press 重新读取: the host
/// reloads and passes it on to the app. What to tell the user about it.
fn tell_running() -> &'static str {
    let told = fibo_core::plugin_context::runtime().map(|r| {
        fibo_core::protocol::rpc(
            &r.join("workbench.sock"),
            &fibo_core::protocol::Request::ReloadPlugins,
        )
    });
    match told {
        Some(Ok(_)) => t!("运行中的 Fibo 已经用上了"),
        _ => t!("Fibo 没在运行，下次打开时生效"),
    }
}

pub fn run(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    let mut args = args.by_ref().peekable();
    if let Some(command) = args
        .peek()
        .filter(|a| {
            matches!(
                a.as_str(),
                "index"
                    | "available"
                    | "install"
                    | "uninstall"
                    | "enable"
                    | "disable"
                    | "keygen"
                    | "log"
                    | "set"
                    | "run"
            )
        })
        .cloned()
    {
        args.next();
        return market_cmd(&command, args);
    }
    let mut dir = None;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" => dir = args.next().map(PathBuf::from),
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_text());
                return Ok(());
            }
            _ => return Err(io::Error::other(usage_text())),
        }
    }
    let dir = match dir {
        Some(dir) => dir,
        None => plugin::dir(
            &fibo_core::agent::config_dir()
                .ok_or_else(|| io::Error::other(t!("找不到配置目录：HOME 没有设置")))?,
        ),
    };
    let home = fibo_platform::dirs::home().unwrap_or_default();
    let checked = check(&dir, &home);
    let bad = checked
        .iter()
        .any(|c| c.error.is_some() || c.files.iter().any(|f| f.error.is_some()));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&checked).map_err(io::Error::other)?
        );
    } else {
        print!("{}", render(&dir, &checked));
    }
    if bad {
        std::process::exit(1);
    }
    Ok(())
}

/// Every plugin through the loaders the way a running Fibo reads them: all the enabled
/// plugins' files of a kind together, so what only shows between two plugins shows here too.
/// A turned-off plugin is read on its own: it would load if it were on.
fn check(dir: &Path, home: &Path) -> Vec<Checked> {
    let found = plugin::discover(dir);
    let enabled: Vec<&plugin::Plugin> = found
        .iter()
        .filter_map(|f| f.result.as_ref().ok())
        .filter(|p| p.enabled)
        .collect();
    let mut outcomes: std::collections::HashMap<PathBuf, File> = Default::default();
    for kind in KINDS {
        let files: Vec<PathBuf> = enabled
            .iter()
            .filter(|p| p.provides.contains(&kind))
            .flat_map(|p| kind.files(&p.dir))
            .collect();
        outcomes.extend(
            load(kind, files, home)
                .into_iter()
                .map(|f| (f.file.clone(), f)),
        );
    }
    found
        .into_iter()
        .map(|found| {
            let undeclared = |provides: &[Kind]| {
                KINDS
                    .into_iter()
                    .filter(|k| !provides.contains(k))
                    .flat_map(|k| k.files(&found.dir))
                    .map(|f| name(&found.dir, &f))
                    .collect()
            };
            match found.result {
                Err(error) => Checked {
                    unused: vec![],
                    warnings: vec![],
                    dir: found.dir,
                    id: None,
                    name: None,
                    enabled: false,
                    error: Some(error),
                    files: vec![],
                },
                Ok(p) => Checked {
                    unused: undeclared(&p.provides),
                    warnings: p.warnings.clone(),
                    files: {
                        let mut files = Vec::new();
                        for k in &p.provides {
                            for f in k.files(&p.dir) {
                                match outcomes.remove(&f) {
                                    Some(outcome) => files.push(outcome),
                                    None => files.extend(load(*k, vec![f], home)),
                                }
                            }
                        }
                        files
                    },
                    dir: p.dir,
                    id: Some(p.id),
                    name: Some(p.name),
                    enabled: p.enabled,
                    error: None,
                },
            }
        })
        .collect()
}

/// One kind's files through the loader the app uses.
fn load(kind: Kind, files: Vec<PathBuf>, home: &Path) -> Vec<File> {
    let report = plugin::load_kind(kind, files, home);
    report
        .into_iter()
        .map(|l| {
            let (ok, error) = match l.result {
                Ok(ok) => (Some(ok), None),
                Err(e) => (None, Some(e)),
            };
            File {
                kind: kind.label(),
                file: l.file,
                ok,
                error,
            }
        })
        .collect()
}

fn name(dir: &Path, file: &Path) -> String {
    file.strip_prefix(dir).unwrap_or(file).display().to_string()
}

fn render(dir: &Path, checked: &[Checked]) -> String {
    let mut out = String::new();
    if checked.is_empty() {
        out.push_str(&format!(
            "{}\n",
            t!("{dir} 下没有插件", dir = dir.display())
        ));
        return out;
    }
    for c in checked {
        let folder = c.dir.file_name().unwrap_or_default().to_string_lossy();
        match (&c.error, &c.id) {
            (Some(error), _) => {
                out.push_str(&format!("✗ {}\n", t!("{folder}：{error}", folder, error)))
            }
            (None, Some(id)) => {
                let bad = c.files.iter().any(|f| f.error.is_some());
                let off = if c.enabled {
                    ""
                } else {
                    t!("（设置里关掉了）")
                };
                let mark = if bad { "✗" } else { "✓" };
                let plugin_name = c.name.as_deref().unwrap_or_default();
                out.push_str(&format!(
                    "{mark} {}\n",
                    t!("{id}「{name}」{off}", id, name = plugin_name, off)
                ));
                for w in &c.warnings {
                    out.push_str(&format!("    ! {w}\n"));
                }
                for f in &c.files {
                    let file = name(&c.dir, &f.file);
                    match (&f.ok, &f.error) {
                        (_, Some(e)) => out.push_str(&format!(
                            "    ✗ {}\n",
                            t!("{kind} {file}：{e}", kind = f.kind, file, e)
                        )),
                        (Some(ok), None) => out.push_str(&format!(
                            "    ✓ {}\n",
                            t!("{kind} {file}：{ok}", kind = f.kind, file, ok)
                        )),
                        (None, None) => {}
                    }
                }
                for file in &c.unused {
                    out.push_str(&format!(
                        "    ! {}\n",
                        t!(
                            "{file} 不会加载：plugin.json 的 provides 里没写它的类型",
                            file
                        )
                    ));
                }
            }
            (None, None) => {}
        }
    }
    out.push_str(&format!(
        "{}\n",
        t!("改完在 Fibo 的 设置 › 插件 点「重新读取」就生效。")
    ));
    out
}

#[cfg(test)]
#[path = "plugins_cmd_tests.rs"]
mod tests;
