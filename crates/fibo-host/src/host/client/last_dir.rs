//! Where a space's terminals last were: closing one remembers its directory, so a new tab
//! in a space with no folder of its own starts there instead of in home.
use super::*;

/// Records, for each closed terminal's space, the directory it was in. Home is skipped:
/// an unpinned space starts there anyway, and it would only erase a remembered project.
pub(super) fn remember(host: &Host, stopped: &[String], next: &mut Workspace) {
    let home = fibo_platform::dirs::home_str();
    for id in stopped {
        let Some(space) = host.workspace.terminal_home(id).map(str::to_owned) else {
            continue;
        };
        let Some(dir) = dir_of(host, id) else {
            continue;
        };
        if home.as_deref() == Some(dir.trim_end_matches('/')) || dir == "/" {
            continue;
        }
        if next.spaces.iter().any(|s| s.id == space) {
            next.space_last_cwd.insert(space, dir);
        }
    }
}

/// The shell's own report (OSC 7), else its process directory while it runs, else the
/// directory it was started in. Only an existing directory counts.
fn dir_of(host: &Host, id: &str) -> Option<String> {
    let terminal = host.workspace.terminals.iter().find(|t| t.id == id)?;
    let reported = host.terminals.get(id).and_then(|live| {
        let live = lock(live).ok()?;
        let pwd = live.vt.pwd(&hostname());
        pwd.or_else(|| {
            (!live.exited)
                .then(|| terminal.pid.and_then(fibo_platform::inspect::cwd))
                .flatten()
        })
    });
    reported
        .into_iter()
        .chain(Some(terminal.cwd.clone()))
        .find(|d| !d.is_empty() && Path::new(d).is_dir())
}
