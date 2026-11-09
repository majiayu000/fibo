//! What the `claude-quota` plugin runs: `mooring claude-quota <what>`. Claude Code's status line
//! is the one thing that hands out its limits and how full each conversation is, so `setup`
//! wraps it, `status` is the wrapper, and `usage` asks Claude Code's own `/usage` when the user
//! turned that on. Everything lives in the plugin's data directory, in the files the scripts
//! this replaces kept, so an install of either version can take over from the other.
mod settings;
mod status;
mod usage;
mod usage_text;

use std::io;
use std::path::PathBuf;

/// Runs before the rest of the command line is set up: a status line runs this every few
/// seconds, and neither it nor the others read the plugin registry.
pub fn run(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    match args.next().as_deref() {
        Some("status") => status::run(PathBuf::from(args.next().ok_or_else(usage_error)?)),
        Some("usage") => usage::run(&data_dir()?),
        Some("setup") => settings::setup(&data_dir()?),
        Some("teardown") => settings::teardown(&data_dir()?),
        _ => Err(usage_error()),
    }
}

fn usage_error() -> io::Error {
    io::Error::other("用法：fibo claude-quota status <数据目录> | usage | setup | teardown")
}

/// Mooring hands a plugin command its data directory.
fn data_dir() -> io::Result<PathBuf> {
    let dir = std::env::var_os("MOORING_PLUGIN_DATA")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("要由 Fibo 运行：缺少 MOORING_PLUGIN_DATA"))?;
    mooring_platform::fs::create_private_dir(&dir)?;
    Ok(dir)
}
