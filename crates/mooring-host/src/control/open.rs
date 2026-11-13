//! `mooring split` and `close`: opening a pane beside a terminal or in its side tab, and closing one.
use super::*;

/// A tab holds at most this many panes opened from outside the app; past it, or when a
/// split would leave a half too small, the pane goes to a side tab instead.
const MAX_PANES: usize = 4;
/// Side tabs are named after the terminal they serve: `side~<terminal>`, then `~2`, `~3`.
const SIDE: &str = mooring_core::model::SIDE_TAB;

/// Where a new pane goes: beside a pane, or into a side tab that does not exist yet.
enum Placement {
    Beside {
        target: String,
        axis: Axis,
        share: u16,
        cols: u16,
        rows: u16,
    },
    NewTab {
        id: String,
        title: String,
    },
}
fn split_room(socket: &Path, target: &str, axis: Axis, share: u16) -> io::Result<(u16, u16)> {
    match rpc(
        socket,
        &Request::SplitRoom {
            target: target.to_owned(),
            axis,
            share,
        },
    )? {
        Reply::Size { cols, rows } => Ok((cols, rows)),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
/// The terminal a tab's side tabs belong to: its top-left pane, where the user started.
/// A side pane split again fills the same side tabs rather than starting a chain.
fn side_owner(w: &Workspace, tab: &Tab, terminal: &str) -> String {
    if let Some(owner) = tab.id.strip_prefix(SIDE).and_then(|r| r.split('~').next()) {
        return owner.to_owned();
    }
    w.tab_terminals(&tab.id)
        .into_iter()
        .next()
        .unwrap_or_else(|| terminal.to_owned())
}
/// The first side tab of `owner` with room for one more pane, beside its last pane,
/// right if that fits and down otherwise; or the next side tab to open.
fn side_placement(
    socket: &Path,
    w: &Workspace,
    owner: &str,
    command: &[String],
) -> io::Result<Placement> {
    // Named for what it runs, not for who asked: `fibo run codex` from Claude opens
    // 「Codex 的旁屏」; a plain shell opens 「终端的旁屏」.
    let name = launched_agent(command)
        .map(|g| g.name().to_owned())
        .unwrap_or_else(|| t!("终端").into());
    // 「Claude 的旁屏」, 「终端的旁屏」.
    let ascii = name.ends_with(|c: char| c.is_ascii());
    for n in 1.. {
        let id = if n == 1 {
            format!("{SIDE}{owner}")
        } else {
            format!("{SIDE}{owner}~{n}")
        };
        let Some(tab) = w.tabs.iter().find(|tab| tab.id == id) else {
            let title = match (ascii, n) {
                (true, 1) => t!("{name} 的旁屏", name),
                (true, _) => t!("{name} 的旁屏 {n}", name, n),
                (false, 1) => t!("{name}的旁屏", name),
                (false, _) => t!("{name}的旁屏 {n}", name, n),
            };
            return Ok(Placement::NewTab { id, title });
        };
        let panes = w.tab_terminals(&tab.id);
        let Some(last) = panes.last().filter(|_| panes.len() < MAX_PANES) else {
            continue;
        };
        for axis in [Axis::Right, Axis::Down] {
            if let Ok((cols, rows)) = split_room(socket, last, axis, 5000) {
                return Ok(Placement::Beside {
                    target: last.clone(),
                    axis,
                    share: 5000,
                    cols,
                    rows,
                });
            }
        }
    }
    unreachable!("there is always a next side tab")
}
/// The Agent a pane's command starts, by the first word naming a registered CLI:
/// `OPENCODE_SERVER_PASSWORD=… opencode --port 0` and `command claude …` both count.
fn launched_agent(command: &[String]) -> Option<mooring_core::agent::Agent> {
    command
        .iter()
        .flat_map(|part| part.split_whitespace())
        .find_map(mooring_core::agent::Agent::from_executable)
}
/// A pane opened beside a terminal, or in its side tab.
pub(super) struct Opened {
    pub(super) id: String,
    pub(super) tab: Option<String>,
    pub(super) tab_title: String,
    pub(super) beside: Option<String>,
    /// Why it went to a side tab instead of beside, when it did.
    pub(super) overflow: Option<String>,
}
pub(super) fn split(a: Args) -> io::Result<()> {
    let opened = open(&a, a.arg(0, t!("终端"))?, &a.rest, a.has("tab"))?;
    if a.has("json") {
        println!(
            "{}",
            serde_json::json!({
                "id": opened.id,
                "tab": opened.tab,
                "tab_title": opened.tab_title,
                "beside": opened.beside,
                "overflow": opened.overflow,
            })
        );
    } else {
        println!("{}", opened.id);
        if let Some(why) = &opened.overflow {
            // On stderr, so `id=$(mooring split …)` still captures only the id.
            eprintln!(
                "{}",
                t!(
                    "{why}，开在了「{tab}」标签（不切换用户的画面）",
                    why,
                    tab = opened.tab_title
                )
            );
        }
    }
    Ok(())
}
/// Opens a shell beside `needle`'s terminal (or in its side tab when `to_tab`, when that
/// tab is full, or when a split would leave a half too small) running `command`.
pub(super) fn open(a: &Args, needle: &str, command: &[String], to_tab: bool) -> io::Result<Opened> {
    if a.has("here") && to_tab {
        return Err(io::Error::other(t!("--here 和 --tab 只能选一个")));
    }
    let w = workspace(&a.socket)?;
    let target = resolve(&w, needle)?;
    let source = w
        .terminals
        .iter()
        .find(|term| term.id == target)
        .ok_or_else(|| io::Error::other(t!("终端不存在")))?;
    let tab = w
        .terminal_tab(&target)
        .ok_or_else(|| io::Error::other(t!("这个终端没有摆在任何画面里，不能在它旁边分屏")))?;
    let share = match a.value("size") {
        None => 5000,
        Some(v) => {
            let f: f64 = v
                .parse()
                .map_err(|_| io::Error::other(t!("--size 需要 0 到 1 之间的小数")))?;
            if !(f > 0.0 && f < 1.0) {
                return Err(io::Error::other(t!("--size 需要 0 到 1 之间的小数")));
            }
            ((f * 10000.0).round() as u16).clamp(1, 9999)
        }
    };
    let axis = if a.has("down") {
        Axis::Down
    } else {
        Axis::Right
    };
    // Asked before anything starts, so a refused split leaves nothing behind; a pane
    // beside another starts at the size it will show at.
    let panes = w.tab_terminals(&tab.id).len();
    let (placement, why) = if to_tab {
        (None, t!("按 --tab").to_owned())
    } else if panes >= MAX_PANES {
        let why = t!("这个标签已经有 {panes} 块", panes);
        if a.has("here") {
            return Err(io::Error::other(t!(
                "{why}，不再往里加；去掉 --here 会开在旁屏标签",
                why
            )));
        }
        (None, why)
    } else {
        match split_room(&a.socket, &target, axis, share) {
            Ok((cols, rows)) => (
                Some(Placement::Beside {
                    target: target.clone(),
                    axis,
                    share,
                    cols,
                    rows,
                }),
                String::new(),
            ),
            Err(e) if a.has("here") => return Err(e),
            Err(e) => (None, e.to_string()),
        }
    };
    let overflow = placement.is_none();
    let placement = match placement {
        Some(p) => p,
        None => side_placement(&a.socket, &w, &side_owner(&w, tab, &target), command)?,
    };
    let cwd = match a.value("cwd") {
        Some(dir) => mooring_platform::fs::canonical(Path::new(dir))
            .map_err(|e| io::Error::other(t!("--cwd {dir}：{e}", dir, e)))?
            .to_string_lossy()
            .into_owned(),
        // Where that shell is now, as the workbench's own split does.
        None => match rpc(&a.socket, &Request::TerminalCwd { id: target.clone() }) {
            Ok(Reply::Cwd(dir)) if Path::new(&dir).is_dir() => dir,
            _ => source.cwd.clone(),
        },
    };
    let argv = split_argv(&mooring_platform::shell::interactive(), command);
    let id = format!(
        "split-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    // A new tab's pane is sized by the workbench once that tab is shown.
    let (cols, rows) = match &placement {
        Placement::Beside { cols, rows, .. } => (*cols, *rows),
        Placement::NewTab { .. } => (100, 30),
    };
    match rpc(
        &a.socket,
        &Request::Launch {
            id: id.clone(),
            space: source.space.clone(),
            argv,
            cwd,
            cols,
            rows,
            scheme: Default::default(),
            colors: None,
        },
    )? {
        Reply::Workspace(_) => (),
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
    let (edit, beside) = match placement {
        Placement::Beside {
            target,
            axis,
            share,
            ..
        } => (
            Edit::SplitPane {
                target: target.clone(),
                terminal: id.clone(),
                axis,
                before: false,
                share,
                forced: false,
            },
            Some(target),
        ),
        Placement::NewTab { id: tab, title } => (
            Edit::CreateTab {
                tab: Tab {
                    id: tab,
                    space: source.space.clone(),
                    default_task: None,
                    title,
                    layout: Some(Layout::Leaf(id.clone())),
                },
            },
            None,
        ),
    };
    let place = Request::Edit {
        request: Some(RequestId::fresh()),
        edit,
    };
    if let Err(e) = deliver(&a.socket, &place, PATIENCE).into_result() {
        // Nothing shows it, so nothing would ever stop it.
        let _ = rpc(&a.socket, &Request::Stop { id });
        return Err(e);
    }
    let after = workspace(&a.socket)?;
    let placed = after.terminal_tab(&id);
    Ok(Opened {
        tab: placed.map(|tab| tab.id.clone()),
        tab_title: after.terminal_tab_title(&id),
        id,
        beside,
        overflow: overflow.then_some(why),
    })
}
/// A login shell, or one that runs `command` first and stays open after it, the way a
/// resumed conversation starts, so its output stays readable and the pane stays usable.
pub(super) fn split_argv(shell: &str, command: &[String]) -> Vec<String> {
    use mooring_platform::shell;
    let command = match command {
        [] => return shell::stay(shell),
        // One argument is a command line already, `&&` and pipes included.
        [line] => line.clone(),
        words => words
            .iter()
            .map(|w| shell::quote(w))
            .collect::<Vec<_>>()
            .join(" "),
    };
    shell::run_then_stay(shell, &command, None)
}
/// Closes a pane as its × does: the tab goes with its last pane, the program ends, and
/// `events` reports `terminal_exited`.
pub(super) fn close(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let needle = a.arg(0, t!("终端"))?;
    // An exited pane still shows; close it by its full id.
    let id = match w.terminal_tab(needle) {
        Some(_) => needle.to_owned(),
        None => resolve(&w, needle)?,
    };
    if w.terminal_tab(&id).is_none() {
        return Err(io::Error::other(t!("这个终端没有摆在任何画面里")));
    }
    let close = Request::Edit {
        request: Some(RequestId::fresh()),
        edit: Edit::ClosePane { terminal: id },
    };
    match deliver(&a.socket, &close, PATIENCE).into_result()? {
        Reply::Workspace(_) => Ok(()),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::launched_agent;
    use mooring_core::agent::Agent;

    /// The side tab is named for what runs in it, not for the Agent that asked.
    #[test]
    fn a_side_tab_is_named_for_the_agent_it_starts() {
        let name = |c: &[&str]| {
            let c: Vec<String> = c.iter().map(|s| s.to_string()).collect();
            launched_agent(&c).map(Agent::name)
        };
        assert_eq!(
            name(&[Agent::Codex.command().as_str()]),
            Some(Agent::Codex.name())
        );
        assert_eq!(
            name(&["command claude --permission-mode default"]),
            Some(Agent::Claude.name())
        );
        assert_eq!(
            name(&["X=$(uuidgen) codex --port 0"]),
            Some(Agent::Codex.name())
        );
        assert_eq!(name(&["npm", "run", "dev"]), None);
        assert_eq!(name(&[]), None);
    }
}
