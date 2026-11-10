//! Whether the status bar says so when a CLI running here has no hook of ours, or an old
//! one. Kept in `<config>/hook-hint` as "off"; on until switched off in 设置 › 通用.
use gpui::{App, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HookHint(pub bool);
impl Global for HookHint {}
impl HookHint {
    /// A file that cannot be read keeps the reminder: it is one quiet item, and silence
    /// here is how a missing hook goes unnoticed.
    pub fn load() -> Self {
        Self(
            path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .is_none_or(|text| text.trim() != "off"),
        )
    }
}
pub fn reminded(cx: &App) -> bool {
    cx.try_global::<HookHint>().is_none_or(|h| h.0)
}
pub fn choose(on: bool, cx: &mut App) -> Result<(), String> {
    let path = path().ok_or_else(|| t!("找不到配置目录").to_string())?;
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, if on { "on" } else { "off" }))
        .map_err(|e| t!("无法保存 {path}：{e}", path = path.display(), e))?;
    cx.set_global(HookHint(on));
    Ok(())
}
fn path() -> Option<std::path::PathBuf> {
    super::status_colors::config_dir().map(|dir| dir.join("hook-hint"))
}
