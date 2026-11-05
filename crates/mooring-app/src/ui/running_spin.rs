//! Whether the mark of a running session turns in the sidebar. Off until chosen: turning
//! draws the window about thirty more times a second, which some people want and others
//! would rather not pay for. Kept in `~/.config/mooring/running-spin` as `on` or `off`.
use gpui::{App, Global};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunningSpin(pub bool);
impl Global for RunningSpin {}
impl RunningSpin {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }
    fn parse(text: &str) -> Self {
        Self(text.trim() == "on")
    }
    fn text(self) -> &'static str {
        if self.0 { "on" } else { "off" }
    }
}
pub fn current(cx: &App) -> bool {
    cx.try_global::<RunningSpin>()
        .copied()
        .unwrap_or_default()
        .0
}
pub fn choose(on: bool, cx: &mut App) -> Result<(), String> {
    let path = path().ok_or("找不到配置目录")?;
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, RunningSpin(on).text()))
        .map_err(|e| format!("无法保存 {}：{e}", path.display()))?;
    cx.set_global(RunningSpin(on));
    Ok(())
}
fn path() -> Option<std::path::PathBuf> {
    super::status_colors::config_dir().map(|dir| dir.join("running-spin"))
}

#[cfg(test)]
mod tests {
    use super::RunningSpin;

    #[test]
    fn it_stays_off_unless_the_file_says_on() {
        assert_eq!(RunningSpin::default(), RunningSpin(false));
        for (text, on) in [
            ("on", true),
            ("on\n", true),
            ("off", false),
            ("", false),
            ("yes", false),
        ] {
            assert_eq!(RunningSpin::parse(text), RunningSpin(on), "{text:?}");
        }
        for on in [true, false] {
            assert_eq!(RunningSpin::parse(RunningSpin(on).text()), RunningSpin(on));
        }
    }
}
