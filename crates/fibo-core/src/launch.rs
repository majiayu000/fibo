//! Whether a process was started the way Fibo starts it. A host passes what it inherited
//! to every terminal, and a terminal cannot shed it: a host started inside an ssh login
//! leaves every Claude it starts signed out (2026-11-16, docs/lessons.md).
use fibo_platform::launch::Launch;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchIssue {
    /// Started inside an ssh login, or a session without the screen: nothing started
    /// from it reads the login keychain (Claude reads as signed out; gh, git over https,
    /// code signing and plugin secrets fail).
    RemoteSession,
    /// Started at a lower priority (a shell's `&` adds 5): every terminal runs later.
    LowPriority,
    /// Another program is responsible for it: macOS privacy prompts name that one and
    /// use its permissions, not Fibo's.
    NotFromWindow,
    /// Reported by a newer build than this one.
    #[serde(other)]
    Other,
}

/// What is off about `launch`, for a process `responsible` should be responsible for.
pub fn issues(launch: &Launch, responsible: &Path) -> Vec<LaunchIssue> {
    let mut found = vec![];
    if launch.remote_session == Some(true) || launch.graphic_session == Some(false) {
        found.push(LaunchIssue::RemoteSession);
    }
    if launch.nice > 0 {
        found.push(LaunchIssue::LowPriority);
    }
    if launch
        .responsible
        .as_deref()
        .is_some_and(|p| real(p) != real(responsible))
    {
        found.push(LaunchIssue::NotFromWindow);
    }
    found
}

/// This host's: the window beside it starts it.
pub fn of_host() -> Vec<LaunchIssue> {
    let Ok(exe) = std::env::current_exe() else {
        return vec![];
    };
    let window = exe.with_file_name(format!("fibo-app{}", std::env::consts::EXE_SUFFIX));
    issues(&fibo_platform::launch::current(), &window)
}

/// This window's: opened from the Dock or Finder it is responsible for itself.
pub fn of_window() -> Vec<LaunchIssue> {
    let Ok(exe) = std::env::current_exe() else {
        return vec![];
    };
    issues(&fibo_platform::launch::current(), &exe)
}

/// What the window fixes by starting the host again itself: the host's issues it does not
/// share. Started from a terminal, a window is not responsible for itself either, and a
/// host it starts would not be.
pub fn fixed_by_restart(host: &[LaunchIssue], window: &[LaunchIssue]) -> Vec<LaunchIssue> {
    host.iter()
        .filter(|i| **i != LaunchIssue::Other && !window.contains(i))
        .copied()
        .collect()
}

fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
