pub const HAS_COMMAND_KEY: bool = false;
pub const DRAWS_WINDOW_BUTTONS: bool = cfg!(windows);
pub const FILE_MANAGER: &str = if cfg!(windows) {
    "资源管理器"
} else {
    "文件管理器"
};
pub const FILE_MANAGER_EN: &str = if cfg!(windows) {
    "File Explorer"
} else {
    "file manager"
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

/// Windows PowerShell's toast API, attributed to PowerShell since Mooring registers no app id
/// yet; `notify-send` elsewhere. Title and body travel in the environment, so nothing a
/// conversation says is ever parsed as script.
pub fn notify(title: &str, body: &str, sound: bool) -> std::io::Result<()> {
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("powershell.exe");
        c.args(["-NoProfile", "-NonInteractive", "-Command", TOAST])
            .env("MOORING_TOAST_TITLE", title)
            .env("MOORING_TOAST_BODY", body)
            .env("MOORING_TOAST_SILENT", if sound { "0" } else { "1" });
        hide_console(&mut c);
        c
    } else {
        let mut c = std::process::Command::new("notify-send");
        c.args(["--app-name=Fibo", "--", title, body]);
        c
    };
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

const TOAST: &str = r#"$m=[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]
$x=$m::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$t=$x.GetElementsByTagName('text')
[void]$t.Item(0).AppendChild($x.CreateTextNode($env:MOORING_TOAST_TITLE))
[void]$t.Item(1).AppendChild($x.CreateTextNode($env:MOORING_TOAST_BODY))
if ($env:MOORING_TOAST_SILENT -eq '1') { $a=$x.CreateElement('audio'); $a.SetAttribute('silent','true'); [void]$x.DocumentElement.AppendChild($a) }
$m::CreateToastNotifier('{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($x))"#;

#[cfg(windows)]
fn hide_console(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
fn hide_console(_command: &mut std::process::Command) {}

/// Not read on these systems yet.
pub fn reduce_motion() -> bool {
    false
}

/// Nothing yet: a symbol this leaves out shows as a blank box, and nothing else depends on it.
pub fn register_font(_bytes: &'static [u8]) -> Result<(), &'static str> {
    Ok(())
}

/// The account's language: Windows' display language, elsewhere the locale variables.
pub fn preferred_languages() -> Vec<String> {
    #[cfg(windows)]
    {
        // The primary language of the display language; Chinese is 0x04.
        let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        return vec![if id & 0x3ff == 0x04 { "zh" } else { "en" }.to_string()];
    }
    #[allow(unreachable_code)]
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .map(|v| v.replace('_', "-"))
        .into_iter()
        .collect()
}
