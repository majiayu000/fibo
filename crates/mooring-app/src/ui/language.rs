//! Which language the window speaks: the system's (Chinese for a Chinese system, English for
//! any other) unless chosen in 设置 › 通用. Kept in `~/.config/mooring/language` as `auto`,
//! `zh` or `en`; `MOORING_LANG` set to one of those overrides it for a run (screenshots).
use mooring_core::i18n::{self, Choice};

/// The choice saved in the settings, `Auto` when there is none or it cannot be read.
pub fn saved() -> Choice {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| Choice::from_key(&text))
        .unwrap_or_default()
}

/// Sets the language for this process from the run's override, the saved choice and the
/// system. Called once before the first window and again when the choice changes.
pub fn apply() {
    let choice = std::env::var("MOORING_LANG")
        .ok()
        .and_then(|v| Choice::from_key(&v))
        .unwrap_or_else(saved);
    i18n::set(choice.resolve(&mooring_platform::desktop::preferred_languages()));
}

pub fn choose(choice: Choice) -> Result<(), String> {
    let path = path().ok_or_else(|| t!("找不到配置目录").to_string())?;
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| super::status_colors::write_atomic(&path, choice.key().as_bytes()))
        .map_err(|e| t!("无法保存 {path}：{e}", path = path.display(), e))?;
    apply();
    Ok(())
}

fn path() -> Option<std::path::PathBuf> {
    super::status_colors::config_dir().map(|dir| dir.join("language"))
}
