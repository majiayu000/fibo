//! A process a test starts that must end with the test run, however the run ends. `Drop`
//! covers a pass and a panic; a run killed by a timeout or SIGKILL unwinds nothing, so each
//! child has a watchdog holding the other end of a pipe only this run can write to.
use std::io;
use std::ops::{Deref, DerefMut};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

/// Waits in `cat`, costing nothing, until the run's end of the pipe closes. Each child of
/// the process is a terminal's session leader, so its group goes with it: a program that
/// ignores hangup stays out of reach of the terminal closing.
const WATCHDOG: &str = r#"/bin/cat >/dev/null
for c in $(/usr/bin/pgrep -P "$1"); do kill -9 "-$c" "$c" 2>/dev/null; done
kill -9 "$1" 2>/dev/null"#;

pub trait SpawnTied {
    fn spawn_tied(&mut self) -> io::Result<Tied>;
}
impl SpawnTied for Command {
    fn spawn_tied(&mut self) -> io::Result<Tied> {
        let mut child = self.spawn()?;
        // A group of its own: whatever signals the run's group must not take it along.
        let watchdog = Command::new("/bin/sh")
            .args(["-c", WATCHDOG, "watchdog", &child.id().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn();
        match watchdog {
            Ok(watchdog) => Ok(Tied { child, watchdog }),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(e)
            }
        }
    }
}

/// A `Child` with its watchdog. `kill()` still means the process alone, for tests of what
/// that leaves behind; `kill_tree()` is the cleanup.
pub struct Tied {
    child: Child,
    watchdog: Child,
}
impl Tied {
    /// The process and each child's group, for cleanup before removing what they use.
    pub fn kill_tree(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let kids = Command::new("/usr/bin/pgrep")
                .args(["-P", &self.child.id().to_string()])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
                .unwrap_or_default();
            for pid in kids.lines().filter_map(|l| l.trim().parse::<i32>().ok()) {
                // SAFETY: kill(2) on a live child of a process this test started.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                    libc::kill(pid, libc::SIGKILL);
                }
            }
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
impl Deref for Tied {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}
impl DerefMut for Tied {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}
impl Drop for Tied {
    fn drop(&mut self) {
        // The watchdog goes first: it must never signal a pid that was already reaped.
        // SAFETY: kill(2) on the group of a child not yet reaped.
        unsafe { libc::kill(-(self.watchdog.id() as i32), libc::SIGKILL) };
        let _ = self.watchdog.wait();
        self.kill_tree();
    }
}
