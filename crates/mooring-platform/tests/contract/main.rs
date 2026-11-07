//! What the interface promises, run unchanged on every platform: the cheapest place for a
//! difference between them to show. A test here has no platform branch unless the interface
//! itself says a platform may refuse.
mod clock;
mod dirs;
mod fs;
mod process;
mod shell;

/// A new empty directory for one test.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kl-contract-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
