pub const HAS_COMMAND_KEY: bool = false;
pub const DRAWS_WINDOW_BUTTONS: bool = cfg!(windows);
pub const FILE_MANAGER: &str = if cfg!(windows) {
    "资源管理器"
} else {
    "文件管理器"
};

/// Notepad on Windows, which every install has; elsewhere the desktop's handler for the file.
pub fn open_as_text(path: &std::path::Path) -> std::io::Result<()> {
    let program = if cfg!(windows) {
        "notepad.exe"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map(drop)
}

/// Where each installer puts it on Windows, for this account or for all; otherwise on PATH.
pub fn editor_cli(editor: super::Editor) -> Option<std::path::PathBuf> {
    use super::Editor;
    use std::path::PathBuf;
    let (installed, on_path): (&[(&str, &str)], &[&str]) = if cfg!(windows) {
        match editor {
            Editor::Zed => (
                &[("LOCALAPPDATA", r"Programs\Zed\bin\zed.exe")],
                &["zed.exe"],
            ),
            Editor::VsCode => (
                &[
                    ("LOCALAPPDATA", r"Programs\Microsoft VS Code\bin\code.cmd"),
                    ("ProgramFiles", r"Microsoft VS Code\bin\code.cmd"),
                ],
                &["code.cmd"],
            ),
            Editor::Cursor => (
                &[(
                    "LOCALAPPDATA",
                    r"Programs\cursor\resources\app\bin\cursor.cmd",
                )],
                &["cursor.cmd"],
            ),
        }
    } else {
        let names: &[&str] = match editor {
            Editor::Zed => &["zed", "zeditor"],
            Editor::VsCode => &["code"],
            Editor::Cursor => &["cursor"],
        };
        (&[], names)
    };
    let fixed = installed
        .iter()
        .filter_map(|(var, rest)| Some(PathBuf::from(std::env::var_os(var)?).join(rest)));
    let path = std::env::var_os("PATH").unwrap_or_default();
    let searched: Vec<PathBuf> = std::env::split_paths(&path)
        .flat_map(|dir| on_path.iter().map(move |name| dir.join(name)))
        .collect();
    fixed.chain(searched).find(|p| p.is_file())
}

/// Not read on these systems yet.
pub fn reduce_motion() -> bool {
    false
}

/// Nothing yet: a symbol this leaves out shows as a blank box, and nothing else depends on it.
pub fn register_font(_bytes: &'static [u8]) -> Result<(), &'static str> {
    Ok(())
}
