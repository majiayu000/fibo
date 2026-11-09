//! What the desktop says about how this account wants things to look.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "other.rs")]
mod imp;

/// Whether keyboards here have a key for app commands that programs in a terminal never see, as
/// the Mac's ⌘ is. Elsewhere commands borrow Ctrl+Shift.
pub const HAS_COMMAND_KEY: bool = imp::HAS_COMMAND_KEY;

/// Whether a window with its title bar hidden must draw minimize, maximize and close itself.
pub const DRAWS_WINDOW_BUTTONS: bool = imp::DRAWS_WINDOW_BUTTONS;

/// What this system calls the app that shows files in their folders, as a label names it.
pub const FILE_MANAGER: &str = imp::FILE_MANAGER;
/// The same in English: Finder, File Explorer.
pub const FILE_MANAGER_EN: &str = imp::FILE_MANAGER_EN;

/// Opens a file as plain text in the system's own editor, whatever its extension says.
pub fn open_as_text(path: &std::path::Path) -> std::io::Result<()> {
    imp::open_as_text(path)
}

/// Editors a `file:line` can open in, by the command line tool each installs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    Zed,
    VsCode,
    Cursor,
}

/// Where this editor's command line tool is, if it is installed where it installs itself.
pub fn editor_cli(editor: Editor) -> Option<std::path::PathBuf> {
    imp::editor_cli(editor)
}

/// Shows a notification through the system's own center, for a window with no native one of
/// its own (the Mac's is in the app). The text is handed over as data, never as code.
pub fn notify(title: &str, body: &str, sound: bool) -> std::io::Result<()> {
    imp::notify(title, body, sound)
}

/// The account's preferred languages, most wanted first, as tags like `zh-Hans-CN`; empty
/// when the system does not say.
pub fn preferred_languages() -> Vec<String> {
    imp::preferred_languages()
}

/// Whether the system is set to reduce motion. May take a while: off the main thread.
pub fn reduce_motion() -> bool {
    imp::reduce_motion()
}

/// Makes a font this process carries available to its text system, for faces GPUI's own loader
/// passes over. It must stay loaded for as long as the process runs.
pub fn register_font(bytes: &'static [u8]) -> Result<(), &'static str> {
    imp::register_font(bytes)
}
