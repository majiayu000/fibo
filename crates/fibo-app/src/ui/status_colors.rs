//! The default status and tone colors, what the current theme puts over them, and where
//! Fibo's preferences live. Status colors color sessions only; `Tone` is for everything
//! else that is fine, a warning or a failure. A theme sets them under `status`, `tone` and
//! `seen`; the old `colors.json` is folded into themes once (`color_migration`).
use super::theme::{Status, Theme, mix};
use super::tone::Tone;
pub use fibo_core::theme::SeenDot;

fn index(status: Status) -> usize {
    Status::ALL
        .iter()
        .position(|s| *s == status)
        .expect("ALL lists every status")
}
const YELLOW: u32 = 0xe5b83b;
const GREEN: u32 = 0x3fb27f;
const SLATE: u32 = 0x8a9bb0;
const ORANGE: u32 = 0xe8732c;
const RED: u32 = 0xe5534b;
/// The default hue of each status (2026-11-02): yellow is only ever work in flight,
/// green only ever a finished turn waiting for you; what you marked complete is done
/// with and drops back to the muted tone.
const fn default_hue(status: Status) -> Option<u32> {
    match status {
        Status::Waiting => Some(ORANGE),
        Status::Error => Some(RED),
        Status::Running => Some(YELLOW),
        // Unseen fills the dot, seen draws it as a ring. 等后台's turn ended too: it waits
        // for you with 等你验收, its ◔ tells them apart.
        Status::Review | Status::Background => Some(GREEN),
        // Quiet states share one slate and differ by glyph: ‖ ○ ◷.
        Status::Paused | Status::Fresh | Status::Deferred => Some(SLATE),
        // The muted text tone: 很久没动, and 做完了 once you marked it (its ✓ says so).
        Status::Stale | Status::Closed => None,
    }
}
pub fn hue(status: Status) -> u32 {
    Theme::palette().status[index(status)]
        .or(default_hue(status))
        .unwrap_or_else(Theme::muted)
}
pub fn tone(tone: Tone) -> u32 {
    Theme::palette().tones[tone as usize].unwrap_or([GREEN, ORANGE, RED][tone as usize])
}
pub fn seen_title() -> u32 {
    Theme::palette()
        .seen_title
        .unwrap_or_else(|| mix(Theme::terminal_bg(), Theme::terminal_fg(), 760))
}
pub fn seen_status() -> u32 {
    Theme::palette().seen_status.unwrap_or_else(Theme::muted)
}
pub fn seen_dot() -> SeenDot {
    Theme::palette().seen_dot
}
/// Replace a file whole: a crash or a reader on a timer sees the old one or the new one,
/// never half of either.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = std::path::PathBuf::from(temporary);
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}
pub fn config_dir() -> Option<std::path::PathBuf> {
    fibo_core::agent::config_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replaced_file_leaves_no_temporary_behind() {
        let dir = std::env::temp_dir().join(format!("fibo-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("theme-dark");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::create_dir(dir.join("gone.tmp")).unwrap();
        assert!(
            write_atomic(&dir.join("gone"), b"x").is_err(),
            "a failed write says so"
        );
        assert!(!dir.join("gone").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
    /// Two states that share a hue must differ by glyph, and the loud hues belong to one
    /// state family each: yellow is in flight, green is a finished turn for you to check.
    #[test]
    fn shared_hues_are_told_apart_by_glyph() {
        for (i, a) in Status::ALL.iter().enumerate() {
            for b in &Status::ALL[i + 1..] {
                if default_hue(*a) == default_hue(*b) {
                    assert_ne!(a.glyph(), b.glyph(), "{a:?} {b:?}");
                }
            }
        }
        for s in Status::ALL {
            assert_eq!(default_hue(s) == Some(YELLOW), s.working(), "{s:?}");
            assert_eq!(
                default_hue(s) == Some(GREEN),
                matches!(s, Status::Review | Status::Background),
                "{s:?}"
            );
        }
    }
}
