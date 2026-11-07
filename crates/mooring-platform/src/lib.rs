//! Everything that depends on the operating system, behind one interface.
//!
//! Each capability is a directory: `mod.rs` is the interface and picks an implementation at
//! compile time; the other files are one platform each, and are not compiled anywhere else.
//! So changing `windows.rs` cannot affect a Mac build. The only code every platform shares is
//! a `mod.rs`, and what it promises is written down as `tests/contract`, which all of them run.
//!
//! Above this crate nothing asks which system it is on (`tests/boundary.rs` checks).
//! See docs/platform/index.html.
pub mod clock;
pub mod desktop;
pub mod dirs;
pub mod exit_watch;
pub mod file_watch;
pub mod fs;
pub mod inspect;
#[cfg(unix)]
pub mod peer;
pub mod process;
pub mod shell;
pub mod system;
