//! The commands a CLI runs that nothing lists: found among its child processes, claimed by
//! the log that started them. docs/decisions/2026-11-04-background-shell-own-client.md.
use crate::background_shells::{children, contains};
use std::io;

/// Commands a CLI named `executable` runs, wherever it runs them: `(pid, command line)`
/// for the children of every such process, in a terminal or a shared daemon. Not its MCP
/// servers and helpers, which talk to it over stdin, and not more of the CLI itself.
pub fn commands_of(executable: &str) -> Vec<(u32, String)> {
    let mut out = vec![];
    for cli in all_pids().into_iter().filter(|p| name(*p) == executable) {
        for pid in children(cli) {
            let Ok(argv) = mooring_platform::inspect::argv(pid) else {
                continue;
            };
            let line = argv.join(" ");
            if stdin_is_a_channel(pid) || crate::agents::from_command(&line).is_some() {
                continue;
            }
            out.push((pid, as_logged(&argv)));
        }
    }
    out
}
/// The part of a command line its log writes the same. A shell runs the text it was given;
/// a shell that ran one program became it, and the program may rewrite its own name
/// (`python3` shows as the full path of `Python`), so only the arguments are sure.
fn as_logged(argv: &[String]) -> String {
    match argv {
        [_, flag, text, ..] if flag == "-lc" || flag == "-c" => text.clone(),
        [program] => program.rsplit('/').next().unwrap_or(program).to_owned(),
        [_, args @ ..] => args.join(" "),
        [] => String::new(),
    }
}
/// Whether this log started `command`. Its start is enough, and has no quote the log
/// writes differently; a command too short to tell apart this way is not claimed.
pub fn log_ran(log: &std::path::Path, command: &str) -> io::Result<bool> {
    let head: String = command
        .chars()
        .take_while(|c| !matches!(c, '"' | '\\' | '\'' | '\n' | '$' | '`'))
        .take(60)
        .collect();
    if head.trim().len() < 4 {
        return Ok(false);
    }
    Ok(contains(&std::fs::read(log)?, head.as_bytes()))
}
fn all_pids() -> Vec<u32> {
    let mut buf = vec![0i32; 8192];
    let bytes = (buf.len() * std::mem::size_of::<i32>()) as i32;
    let n = unsafe { libc::proc_listallpids(buf.as_mut_ptr().cast(), bytes) };
    buf[..n.max(0) as usize]
        .iter()
        .filter(|p| **p > 0)
        .map(|p| *p as u32)
        .collect()
}
fn name(pid: u32) -> String {
    let mut buf = [0u8; 256];
    let n = unsafe { libc::proc_name(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
    String::from_utf8_lossy(&buf[..n.max(0) as usize]).into_owned()
}
/// Whether fd 0 is a pipe or a socket, which is how a CLI talks to its MCP servers. A
/// command's stdin is `/dev/null` or a terminal (real Codex 0.167 processes, 2026-11-06).
fn stdin_is_a_channel(pid: u32) -> bool {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FdInfo {
        fd: i32,
        kind: u32,
    }
    const PROC_PIDLISTFDS: i32 = 1;
    const SOCKET: u32 = 2;
    const PIPE: u32 = 6;
    let mut fds = [FdInfo { fd: -1, kind: 0 }; 64];
    let size = std::mem::size_of_val(&fds) as i32;
    let n = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            size,
        )
    };
    let count = n.max(0) as usize / std::mem::size_of::<FdInfo>();
    fds[..count.min(fds.len())]
        .iter()
        .any(|f| f.fd == 0 && matches!(f.kind, SOCKET | PIPE))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_log_claims_a_command_by_its_start_as_it_wrote_it() {
        let dir = std::env::temp_dir().join(format!("kl-ran-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("s.jsonl");
        // Shaped as Codex 0.167 logs `exec`: the command is a string inside a script.
        let row = r#"{"payload":{"name":"exec","input":"text(await tools.exec_command({cmd:\"python3 -m http.server 8000 --bind 127.0.0.1 # \\\"x\\\"\",yield_time_ms:1}));"}}"#;
        std::fs::write(&log, row).unwrap();
        let ran = |c: &str| log_ran(&log, c).unwrap();
        assert!(ran("python3 -m http.server 8000 --bind 127.0.0.1 # \"x\""));
        assert!(!ran("python3 -m http.server 9000"));
        assert!(!ran("ls"), "too short to claim");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    /// Found as a child of a process with this name; with a stdin pipe it is a helper.
    #[test]
    fn a_cli_commands_are_its_children_with_stdin_closed() {
        use std::process::{Command, Stdio};
        let me = name(std::process::id());
        assert!(!me.is_empty());
        let mut command = Command::new("/bin/sh")
            .args(["-c", "sleep 20; echo found-5d2a"])
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let mut helper = Command::new("/bin/sh")
            .args(["-c", "sleep 20; echo helper-5d2a"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let got = commands_of(&me);
        let _ = command.kill();
        let _ = helper.kill();
        let _ = command.wait();
        let _ = helper.wait();
        assert!(
            got.iter().any(|(_, c)| c == "sleep 20; echo found-5d2a"),
            "{got:?}"
        );
        assert!(
            !got.iter().any(|(_, c)| c.contains("helper-5d2a")),
            "{got:?}"
        );
    }
    #[test]
    fn a_command_is_compared_by_what_its_log_writes_the_same() {
        let line = |a: &[&str]| as_logged(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(
            line(&["/bin/zsh", "-lc", "sleep 9; echo a"]),
            "sleep 9; echo a"
        );
        // As Codex 0.167's real http.server looked: zsh became python, python renamed itself.
        assert_eq!(
            line(&["/opt/homebrew/x/Python", "-m", "http.server", "8000"]),
            "-m http.server 8000"
        );
        assert_eq!(line(&["/usr/local/bin/serve"]), "serve");
        assert_eq!(line(&[]), "");
    }
    /// A command's stdin is not a channel; an MCP server's is.
    #[test]
    fn only_a_helper_talks_over_stdin() {
        use std::process::{Command, Stdio};
        let mut helper = Command::new("/bin/sleep")
            .arg("20")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut command = Command::new("/bin/sleep")
            .arg("20")
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        assert!(stdin_is_a_channel(helper.id()));
        assert!(!stdin_is_a_channel(command.id()));
        let _ = helper.kill();
        let _ = command.kill();
        let _ = helper.wait();
        let _ = command.wait();
    }
}
