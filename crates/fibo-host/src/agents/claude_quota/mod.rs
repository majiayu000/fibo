//! HTTP supplies account quota; status lines supply conversation context. Those independent
//! facts never overwrite each other. See the 2026-11-16 quota decision.
mod oauth;
mod settings;
mod status;
mod usage;

use std::io;
use std::path::PathBuf;

/// Runs before the rest of the command line is set up: a status line runs this every few
/// seconds, and neither it nor the others read the plugin registry.
pub fn run(mut args: impl Iterator<Item = String>) -> io::Result<()> {
    match args.next().as_deref() {
        Some("status") => status::run(PathBuf::from(args.next().ok_or_else(usage_error)?)),
        Some("usage") => usage::run(&data_dir()?, false),
        Some("refresh") => usage::run(&data_dir()?, true),
        Some("setup") => settings::setup(&data_dir()?),
        Some("teardown") => settings::teardown(&data_dir()?),
        _ => Err(usage_error()),
    }
}

fn usage_error() -> io::Error {
    io::Error::other(
        "用法：fibo claude-quota status <数据目录> | usage | refresh | setup | teardown",
    )
}

/// Fibo hands a plugin command its data directory.
fn data_dir() -> io::Result<PathBuf> {
    let dir = std::env::var_os("MOORING_PLUGIN_DATA")
        .map(PathBuf::from)
        .or_else(|| fibo_core::agent::config_dir().map(|d| d.join("plugin-data/claude-quota")))
        .ok_or_else(|| io::Error::other("找不到插件数据目录"))?;
    fibo_platform::fs::create_private_dir(&dir)?;
    Ok(dir)
}
