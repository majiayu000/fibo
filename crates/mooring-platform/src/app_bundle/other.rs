//! Systems whose app is not replaced by itself.
use std::path::{Path, PathBuf};

pub const SUPPORTED: bool = false;
const NO: &str = "这个系统上不能自动更新";

pub fn cache_dir() -> Option<PathBuf> {
    None
}
pub fn fetch(_: &str, _: bool, _: u64) -> Result<Vec<u8>, String> {
    Err(NO.into())
}
pub fn download(_: &str, _: &Path, _: bool, _: u64) -> Result<(), String> {
    Err(NO.into())
}
pub fn unzip(_: &Path, _: &Path) -> Result<(), String> {
    Err(NO.into())
}
pub fn info(_: &Path, _: &str) -> Result<String, String> {
    Err(NO.into())
}
pub fn verify_signature(_: &Path, _: &str) -> Result<(), String> {
    Err(NO.into())
}
pub fn team_id(_: &Path) -> Result<String, String> {
    Err(NO.into())
}
pub fn assess(_: &Path) -> Result<(), String> {
    Err(NO.into())
}
pub fn same_owner(_: &Path, _: &Path) -> std::io::Result<bool> {
    Ok(true)
}
pub fn homebrew_prefixes() -> Vec<PathBuf> {
    Vec::new()
}
