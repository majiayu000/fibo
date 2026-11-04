//! `mooring plugins`: checks the plugins under the config directory with the same code
//! the app loads them with, so an Agent that wrote one knows before a restart whether it
//! will take effect, and why not; and installs, removes and configures them.
use mooring_core::market;
use mooring_core::plugin::{self, Kind};
use std::io;
use std::path::{Path, PathBuf};

/// Its lines in `mooring --help`.
pub const COMMAND_USAGE: &str = "\
mooring plugins [--dir 目录] [--json]      检查插件能不能装上：哪个生效、哪个为什么没生效
mooring plugins available [--remote]       列出可安装的插件（App 附带的，加 --remote 再查远程目录）
mooring plugins install <id>[@版本] [--remote] [--yes]
                                             安装或更新一个插件；会跑命令的要确认，非交互时加 --yes
mooring plugins uninstall <id> [--force]   卸载一个插件；--force 在 teardown 失败时也卸
mooring plugins disable|enable <id> [--force]
                                             停用或打开一个插件；--force 在 teardown 失败时也停用
mooring plugins log <id>                   这个插件最近的运行和出过的错
mooring plugins set <id> [<key> <值>]      看或改插件的设置项
mooring plugins run <id> <声明 id> [--event 事件] [--cwd 目录] [--send]
                                             用和 App 一样的环境试跑一条 hook、装饰或额度命令；webhook 加 --send 才真发
mooring plugins index <目录> [--check] [--key 私钥文件]
                                             为插件目录生成 index.json 并签名（--check 只检查是否最新、签名对不对）
mooring plugins keygen [--name 发布者] [--key 私钥文件]
                                             生成发布者的签名私钥，默认 <配置>/publisher.key";
pub const USAGE: &str = "用法：mooring plugins [--dir 插件目录] [--json]
      mooring plugins available [--remote]   列出可安装的插件（App 附带的，加 --remote 再查远程目录）
      mooring plugins install <id>[@版本] [--remote] [--yes]
                                              安装或更新一个插件；会跑命令的要确认，非交互时加 --yes
      mooring plugins uninstall <id> [--force]
                                              卸载一个插件；--force 在 teardown 失败时也卸（加载不了的按目录名卸）
      mooring plugins disable|enable <id> [--force]
                                              停用或打开一个插件；--force 在 teardown 失败时也停用
      mooring plugins index <目录> [--check] [--key 私钥文件]
                                              为插件目录生成 index.json 并用私钥签名（--check 只检查是否最新、签名对不对）
      mooring plugins keygen [--name 发布者] [--key 私钥文件]
                                              生成发布者的签名私钥，默认 <配置>/publisher.key
      mooring plugins log <id>                这个插件最近的运行：setup、hook、出过错的装饰和额度命令
      mooring plugins set <id> [<key> <值>]   看或改插件的设置项；值写 true/false、数字或文字
      mooring plugins run <id> <声明 id> [--event 事件] [--cwd 目录]
                                              用和 App 一样的环境试跑一条 hook、装饰或额度命令，打印输出；
                                              webhook 只显示会发什么，加 --send 才真发

检查插件能不能装上：逐个读 plugin.json 和它提供的文件，说哪个生效、哪个为什么没生效。
默认目录是 ${XDG_CONFIG_HOME:-$HOME/.config}/mooring/plugins。有问题时以 1 退出。
手写改了插件目录后，在 Mooring 的 设置 › 插件 点「重新读取」就生效。
写插件的说明：mooring --skill plugins";

/// The first line of the `AGENTS.md` Mooring writes; a file without it is the user's.
const MARK: &str =
    "<!-- 由 Mooring 生成，每次启动会更新；要写自己的说明就删掉这一行，Mooring 就不再覆盖 -->";
const AGENTS_MD: &str = "# Mooring 配置目录

这里是 Mooring 的配置。用户让你改 Mooring 时（加右键菜单或 ⌘P 里的命令、换左栏排法、在窗格上显示点什么、会话等你或做完时发通知、换完成动画、改状态颜色）：

1. 先运行 `mooring --skill plugins`，读完整说明：插件格式、规矩和例子。
2. 插件写在 `plugins/<id>/`，一个想法一个插件。写完运行 `mooring plugins` 检查，全是 ✓ 才算写好。
3. 告诉用户在 Mooring 的 设置 › 插件 点「重新读取」就生效，以及去哪里看。

别改用户已有的插件和 `plugins/disabled.json`；快捷键、终端字体、完成动画在 App 的「设置」里改。
在 Mooring 打开的终端里 `mooring` 直接能用；驱动别的终端的说明是 `mooring --skill`。
";

/// Writes `<config>/AGENTS.md`, the note an Agent finds when it opens the config
/// directory, unless the user wrote their own there.
pub fn write_agents_md() {
    let Some(dir) = mooring_core::agent::config_dir() else {
        return;
    };
    if let Err(e) = write_note(&dir) {
        eprintln!("AGENTS.md: {e}");
    }
}
fn write_note(dir: &Path) -> io::Result<()> {
    let path = dir.join("AGENTS.md");
    let want = format!("{MARK}\n{AGENTS_MD}");
    match std::fs::read_to_string(&path) {
        Ok(text) if text == want || !text.starts_with(MARK) => return Ok(()),
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::fs::create_dir_all(dir)?;
    let temporary = path.with_extension("md.tmp");
    std::fs::write(&temporary, want)?;
    std::fs::rename(temporary, path)
}

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
        s if s < 60 => format!("{s} 秒"),
        s if s < 3600 => format!("{} 分", s / 60),
        s if s < 86_400 => format!("{} 小时", s / 3600),
        s => format!("{} 天", s / 86_400),
    }
}

#[derive(serde::Serialize)]
struct File {
    kind: &'static str,
    file: PathBuf,
    ok: Option<String>,
    error: Option<String>,
}

fn plugins_dir() -> io::Result<PathBuf> {
    Ok(plugin::dir(&mooring_core::agent::config_dir().ok_or_else(
        || io::Error::other("找不到配置目录：HOME 没有设置"),
    )?))
}

/// `<config>/publisher.key`, where a publisher's private key lives by default.
fn default_key() -> io::Result<PathBuf> {
    let config = mooring_core::agent::config_dir()
        .ok_or_else(|| io::Error::other("找不到配置目录：HOME 没有设置"))?;
    Ok(config.join("publisher.key"))
}

/// Writes a file only its owner can read, from the moment it exists.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
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
                return Err(io::Error::other(format!(
                    "{} 已经有了，不覆盖：丢了原来的密钥，已经发出去的插件就没法更新了",
                    path.display()
                )));
            }
            let (secret, public) = market::new_key(publisher.as_deref().unwrap_or("未命名"))
                .map_err(io::Error::other)?;
            write_private(&path, secret.as_bytes())?;
            println!(
                "私钥：{}（保管好，别进仓库）\n公钥：{public}",
                path.display()
            );
            Ok(())
        }
        ("index", [dir]) => {
            let dir = Path::new(dir);
            let text = market::render(&market::index_of(dir).map_err(io::Error::other)?);
            let path = dir.join("index.json");
            let sig_path = dir.join("index.sig");
            let same = std::fs::read_to_string(&path).is_ok_and(|t| t == text);
            if check_only {
                if !same {
                    eprintln!("{} 过期了", path.display());
                    std::process::exit(1);
                }
                match std::fs::read(&sig_path) {
                    Ok(sig) => match market::verify_index(text.as_bytes(), &sig) {
                        Ok(who) => {
                            println!("{} 是最新的，签名有效：{}", path.display(), who.said())
                        }
                        Err(why) => {
                            eprintln!("{}：{why}", sig_path.display());
                            std::process::exit(1);
                        }
                    },
                    Err(_) => println!("{} 是最新的，没有签名", path.display()),
                }
                return Ok(());
            }
            if same {
                println!("{} 是最新的", path.display());
            } else {
                std::fs::write(&path, &text)?;
                println!("写好了 {}", path.display());
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
                    println!("签好了 {}", sig_path.display());
                }
                None if sig_path.exists() && !same => {
                    eprintln!(
                        "注意：没有私钥，{} 已经和新的目录对不上",
                        sig_path.display()
                    );
                }
                None => {}
            }
            Ok(())
        }
        ("available", []) => {
            let installed = plugin::discover(&plugins_dir()?);
            for o in offers(remote)? {
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
                        format!("（已装 v{}，可更新）", p.version.as_deref().unwrap_or("?"))
                    }
                    Some(_) => "（已安装）".into(),
                };
                println!(
                    "{} v{}「{}」{state} · {}\n    {}",
                    e.id,
                    e.version,
                    e.name,
                    o.origin(),
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
                .ok_or_else(|| io::Error::other(format!("目录里没有叫 {id} 的插件")))?;
            let offer = match version {
                Some(v) => offer
                    .pick(v)
                    .ok_or_else(|| io::Error::other(format!("目录里 {id} 没有 {v} 这个版本")))?,
                None => offer,
            };
            // Read from the package itself, before anything installed changes.
            let prepared = offer.prepare(&plugins_dir()?).map_err(io::Error::other)?;
            println!("{id} v{} 装上后会：", offer.entry.version);
            for p in prepared.lines() {
                println!("  · {p}");
            }
            // An Agent or a script installing one that runs commands says so on purpose,
            // and what it will do is now in its output for the user to read.
            use std::io::IsTerminal;
            if prepared.must_ask && !yes {
                if !std::io::stdin().is_terminal() {
                    return Err(io::Error::other("先确认上面这些，再加 --yes 安装"));
                }
                eprint!("确认安装？[y/N] ");
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim(), "y" | "Y" | "yes") {
                    return Err(io::Error::other("没有安装"));
                }
            }
            let result = prepared.commit().map_err(io::Error::other);
            // Even a failed one may have changed what is installed.
            let told = tell_running();
            let at = result?;
            println!("装好了 {id} v{} → {}", offer.entry.version, at.display());
            println!("{told}");
            Ok(())
        }
        ("uninstall", [id]) => {
            let warned =
                plugin::uninstall_in(&plugins_dir()?, id, force).map_err(io::Error::other)?;
            println!("卸载了 {id}");
            if let Some(w) = warned {
                println!("注意：{w}");
            }
            println!("{}", tell_running());
            Ok(())
        }
        (verb @ ("enable" | "disable"), [id]) => {
            let warned = plugin::set_enabled_in(&plugins_dir()?, id, verb == "enable", force)
                .map_err(io::Error::other)?;
            println!("{}了 {id}", if verb == "enable" { "打开" } else { "停用" });
            if let Some(w) = warned {
                println!("注意：{w}");
            }
            println!("{}", tell_running());
            Ok(())
        }
        ("run", [id, declaration]) => try_run(id, declaration, event, cwd, send),
        ("log", [id]) => {
            let runs = mooring_core::plugin_context::runs(&plugins_dir()?, id);
            if runs.is_empty() {
                println!("{id} 还没有运行记录");
            }
            let now = mooring_core::content::now();
            for r in runs {
                println!(
                    "{} {:>6} 前  {:<28} {:>5} ms  {}",
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
                .ok_or_else(|| io::Error::other(format!("没有装叫 {id} 的插件")))?;
            match rest {
                [] => {
                    if p.options.is_empty() {
                        println!("{id} 没有设置项");
                    }
                    for (o, v) in mooring_core::plugin_context::values(&dir, &p) {
                        println!(
                            "{} = {}    {}{}",
                            o.key,
                            if o.secret && v == serde_json::Value::String(String::new()) {
                                "未设置".to_owned()
                            } else {
                                mooring_core::plugin_context::Opt::text(&v)
                            },
                            o.title,
                            o.description.map(|d| format!("：{d}")).unwrap_or_default()
                        );
                    }
                    Ok(())
                }
                [key, value] => {
                    let opt =
                        p.options.iter().find(|o| &o.key == key).ok_or_else(|| {
                            io::Error::other(format!("{id} 没有叫 {key} 的设置项"))
                        })?;
                    // A command line is in the shell's history and in `ps`: a secret comes on
                    // standard input, and is never printed back.
                    let (v, shown) = if opt.secret {
                        if value != "-" {
                            return Err(io::Error::other(format!(
                                "{key} 是密钥，别写在命令行里：printf %s 值 | mooring plugins set {id} {key} -"
                            )));
                        }
                        let mut text = String::new();
                        io::Read::read_to_string(&mut io::stdin(), &mut text)?;
                        let text = text.trim_end_matches(['\n', '\r']).to_owned();
                        let said = if text.is_empty() {
                            "清空了"
                        } else {
                            "存进了钥匙串"
                        };
                        (serde_json::Value::String(text), said.to_owned())
                    } else {
                        (
                            opt.parse(value).map_err(io::Error::other)?,
                            format!("改成了 {value}"),
                        )
                    };
                    mooring_core::plugin_context::set(&dir, id, key, v)
                        .map_err(io::Error::other)?;
                    println!("{id} 的 {key} {shown}，下次运行就用");
                    Ok(())
                }
                _ => Err(io::Error::other(USAGE)),
            }
        }
        _ => Err(io::Error::other(USAGE)),
    }
}

/// Tells a running Mooring what changed, so nobody has to go and press 重新读取: the host
/// reloads and passes it on to the app. What to tell the user about it.
fn tell_running() -> String {
    let told = mooring_core::plugin_context::runtime().map(|r| {
        mooring_core::protocol::rpc(
            &r.join("workbench.sock"),
            &mooring_core::protocol::Request::ReloadPlugins,
        )
    });
    match told {
        Some(Ok(_)) => "运行中的 Mooring 已经用上了".to_owned(),
        _ => "Mooring 没在运行，下次打开时生效".to_owned(),
    }
}

/// Runs one declared command as the app or the host would, and shows everything about how
/// it went: `mooring plugins` only says that a file reads, this says the command works.
fn try_run(
    id: &str,
    declaration: &str,
    event: Option<String>,
    cwd: Option<String>,
    send: bool,
) -> io::Result<()> {
    use mooring_core::plugin_command::{self, Failed};
    use mooring_core::{decoration, hook, quota};
    let p = plugin::discover(&plugins_dir()?)
        .into_iter()
        .find_map(|f| f.result.ok().filter(|p| p.id == id))
        .ok_or_else(|| io::Error::other(format!("没有装叫 {id} 的插件")))?;
    let files = |kind: Kind| kind.files(&p.dir);
    let cwd = match cwd {
        Some(dir) => dir,
        None => std::env::current_dir()?.display().to_string(),
    };
    let (hooks, decorations, sources) = (
        hook::load(files(Kind::Hooks)).0,
        decoration::load(files(Kind::Decorations)).0,
        quota::load(files(Kind::Quota)).0,
    );
    let pane = decoration::Pane {
        cwd: cwd.clone(),
        ..Default::default()
    };
    let spec = if let Some(h) = hooks.iter().find(|h| h.id == declaration) {
        let on = match event {
            Some(name) => serde_json::from_value(serde_json::Value::String(name.clone()))
                .map_err(|_| io::Error::other(format!("没有 {name} 这个事件")))?,
            None => *h.on.first().unwrap_or(&hook::Event::Done),
        };
        let payload = hook::Payload {
            event: on,
            session: "试跑的会话".into(),
            agent: "Claude Code".into(),
            title: "试跑".into(),
            cwd: cwd.clone(),
            space: Some("试跑".into()),
            detail: "这是 mooring plugins run 发的样例事件".into(),
            at: mooring_core::content::now(),
            terminal: None,
            exit_code: on
                .is_command()
                .then_some(if on == hook::Event::CommandFailed {
                    1
                } else {
                    0
                }),
            duration_ms: on.is_command().then_some(1200),
        };
        println!(
            "事件：{}",
            serde_json::to_string(&payload).map_err(io::Error::other)?
        );
        if let (Some(url), false) = (&h.webhook, send) {
            println!("会 POST 到 {url}，没有真的发：加 --send 才发");
            return Ok(());
        }
        mooring_host::plugin_hooks::command_for(h, &payload).map_err(io::Error::other)?
    } else if let Some(d) = decorations.iter().find(|d| d.id == declaration) {
        d.spec(&pane)
    } else if let Some(s) = sources.iter().find(|s| s.agent == declaration) {
        if s.command.is_none() {
            return Err(io::Error::other(format!(
                "{declaration} 的额度来自文件 {}，没有命令可跑",
                s.file.as_deref().unwrap_or_default()
            )));
        }
        s.spec()
    } else {
        let known: Vec<String> = hooks
            .iter()
            .map(|h| h.id.clone())
            .chain(decorations.iter().map(|d| d.id.clone()))
            .chain(sources.iter().map(|s| s.agent.clone()))
            .collect();
        return Err(io::Error::other(format!(
            "{id} 没有叫 {declaration} 的 hook、装饰或额度命令；有：{}",
            known.join("、")
        )));
    };
    println!("PATH：{}", plugin_command::path());
    let started = std::time::Instant::now();
    let done = plugin_command::run(spec).map_err(|e| {
        io::Error::other(match e {
            Failed::Start(why) => format!("启动不了：{why}"),
            Failed::TimedOut => {
                format!("超时，已停止（用了 {} ms）", started.elapsed().as_millis())
            }
        })
    })?;
    println!("{}，用了 {} ms", done.how(), started.elapsed().as_millis());
    for (name, bytes) in [("输出", &done.stdout), ("错误输出", &done.stderr)] {
        let text = String::from_utf8_lossy(bytes);
        if !text.trim().is_empty() {
            println!("{name}：\n{}", text.trim_end());
        }
    }
    if done.not_runnable() {
        return Err(io::Error::other(
            "命令跑不起来：在 App 里也一样；检查命令名和上面的 PATH",
        ));
    }
    Ok(())
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
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(io::Error::other(USAGE)),
        }
    }
    let dir = match dir {
        Some(dir) => dir,
        None => plugin::dir(
            &mooring_core::agent::config_dir()
                .ok_or_else(|| io::Error::other("找不到配置目录：HOME 没有设置"))?,
        ),
    };
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
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

/// Every plugin through the loaders the way a running Mooring reads them: all the enabled
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
        out.push_str(&format!("{} 下没有插件\n", dir.display()));
        return out;
    }
    for c in checked {
        let folder = c.dir.file_name().unwrap_or_default().to_string_lossy();
        match (&c.error, &c.id) {
            (Some(error), _) => out.push_str(&format!("✗ {folder}：{error}\n")),
            (None, Some(id)) => {
                let bad = c.files.iter().any(|f| f.error.is_some());
                let off = if c.enabled {
                    ""
                } else {
                    "（设置里关掉了）"
                };
                out.push_str(&format!(
                    "{} {id}「{}」{off}\n",
                    if bad { "✗" } else { "✓" },
                    c.name.as_deref().unwrap_or_default()
                ));
                for w in &c.warnings {
                    out.push_str(&format!("    ! {w}\n"));
                }
                for f in &c.files {
                    let file = name(&c.dir, &f.file);
                    match (&f.ok, &f.error) {
                        (_, Some(e)) => out.push_str(&format!("    ✗ {} {file}：{e}\n", f.kind)),
                        (Some(ok), None) => {
                            out.push_str(&format!("    ✓ {} {file}：{ok}\n", f.kind))
                        }
                        (None, None) => {}
                    }
                }
                for file in &c.unused {
                    out.push_str(&format!(
                        "    ! {file} 不会加载：plugin.json 的 provides 里没写它的类型\n"
                    ));
                }
            }
            (None, None) => {}
        }
    }
    out.push_str("改完在 Mooring 的 设置 › 插件 点「重新读取」就生效。\n");
    out
}

#[cfg(test)]
#[path = "plugins_cmd_tests.rs"]
mod tests;
