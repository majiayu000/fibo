//! `tied.rs` on Windows: a process a test starts ends with the test run, however the run ends.
//! The same idea, a watchdog holding the other end of a pipe only this run can write to. What
//! was in its terminals goes with the host, because their pseudo consoles close.
use std::io;
use std::ops::{Deref, DerefMut};
use std::process::{Child, Command, Stdio};

pub trait SpawnTied {
    fn spawn_tied(&mut self) -> io::Result<Tied>;
}
impl SpawnTied for Command {
    fn spawn_tied(&mut self) -> io::Result<Tied> {
        let mut child = self.spawn()?;
        let script = format!(
            "[void][Console]::In.ReadToEnd(); Stop-Process -Id {} -Force -ErrorAction SilentlyContinue",
            child.id()
        );
        let watchdog = Command::new(fibo_platform::shell::interactive())
            .args(["-NoLogo", "-NoProfile", "-Command", &script])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
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

/// A `Child` with its watchdog.
pub struct Tied {
    child: Child,
    watchdog: Child,
}
impl Tied {
    pub fn kill_tree(&mut self) {
        let _ = self.child.kill();
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
        // The watchdog goes first: it must never name a process id that was already reused.
        let _ = self.watchdog.kill();
        let _ = self.watchdog.wait();
        self.kill_tree();
    }
}
