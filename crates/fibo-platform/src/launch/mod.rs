//! What this process inherited from whatever started it. A long-lived process passes it on
//! to every program it starts, and a program cannot change it afterwards: the session the
//! user's keychain is unlocked in, how soon the scheduler runs it, and which program the
//! system names for its privacy permissions.
#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

use std::path::PathBuf;

/// As far as the system tells; None where it does not say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Launch {
    /// The login session was reached over the network (on macOS, an ssh login). The
    /// user's login keychain is not unlocked in it.
    pub remote_session: Option<bool>,
    /// The session reaches the window server, as a login at the screen does.
    pub graphic_session: Option<bool>,
    /// This process's nice value: 0 is normal, more runs later. Only root lowers it again.
    pub nice: i32,
    /// The program the system holds responsible for this process: macOS privacy prompts
    /// name it and use its permissions.
    pub responsible: Option<PathBuf>,
}

/// This process's launch, read once: none of it changes while it runs.
pub fn current() -> Launch {
    imp::current()
}
