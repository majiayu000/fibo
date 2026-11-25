//! Windows: the release's Inno Setup installer, fetched with the `curl.exe` Windows 10 1803 on
//! ships, run over the install once the window has quit. There is no bundle to read or swap.
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const SUPPORTED: bool = true;
pub const INSTALLER: bool = true;
pub const PAGE_ONLY: bool = false;
const NO: &str = "Windows 上由安装程序更新";
/// No console window flashes up from the window's process.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

pub fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Fibo").join("updates"))
}

fn system32(program: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    Path::new(&root).join("System32").join(program)
}

/// Runs `command`; its stderr (else stdout) is the error.
fn run(mut command: Command, what: &str) -> Result<Output, String> {
    let out = command
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("运行 {what} 失败：{e}"))?;
    if out.status.success() {
        return Ok(out);
    }
    let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_owned();
    Err([text(&out.stderr), text(&out.stdout)]
        .into_iter()
        .find(|t| !t.is_empty())
        .unwrap_or_else(|| format!("{what} 退出码 {:?}", out.status.code())))
}

fn curl(allow_http: bool, max: u64, seconds: u32) -> Command {
    let protocols = if allow_http { "=https,http" } else { "=https" };
    let mut c = Command::new(system32("curl.exe"));
    c.args(["-fsSL", "--proto", protocols, "--proto-redir", protocols])
        .args(["--max-time", &seconds.to_string()])
        .args(["--max-filesize", &max.to_string()])
        .args(["-A", concat!("Fibo/", env!("CARGO_PKG_VERSION"))]);
    c
}

pub fn fetch(url: &str, allow_http: bool, max: u64) -> Result<Vec<u8>, String> {
    let mut c = curl(allow_http, max, 20);
    c.args(["-H", "Accept: application/vnd.github+json"])
        .arg(url);
    Ok(run(c, "curl")?.stdout)
}

pub fn download(url: &str, to: &Path, allow_http: bool, max: u64) -> Result<(), String> {
    let mut c = curl(allow_http, max, 900);
    c.arg("-o").arg(to).arg(url);
    run(c, "curl").map(drop)
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

/// The PowerShell `install_after_exit` runs. The host is stopped first, as the uninstaller
/// does: its files are in use, and a terminal cannot pass to a newer host here anyway.
/// `/LOG` leaves the installer's own account beside the download.
fn script(pid: u32, installer: &Path, dir: &Path, args: &[std::ffi::OsString]) -> String {
    use crate::shell::{literal, quote};
    let bin = dir.join("bin");
    let log = installer.with_extension("log");
    let options = format!(
        "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR=\"{}\" /LOG=\"{}\"",
        dir.display(),
        log.display()
    );
    let app = bin.join("fibo-app.exe");
    let words: Vec<String> = args.iter().map(|a| quote(&a.to_string_lossy())).collect();
    format!(
        "Wait-Process -Id {pid} -ErrorAction Ignore; \
         $bin = {bin}; \
         Get-Process fibo,mooring,fibo-app,mooring-app -ErrorAction Ignore | \
         Where-Object {{ $_.Path -and $_.Path.StartsWith($bin + '\\', 'OrdinalIgnoreCase') }} | \
         Stop-Process -Force; \
         Start-Process -FilePath {setup} -ArgumentList {options} -Wait; \
         Remove-Item -LiteralPath {setup} -ErrorAction Ignore; \
         & {app} {words}",
        bin = literal(&bin.to_string_lossy()),
        setup = literal(&installer.to_string_lossy()),
        options = literal(&options),
        app = quote(&app.to_string_lossy()),
        words = words.join(" "),
    )
}

pub fn install_after_exit(
    pid: u32,
    installer: &Path,
    dir: &Path,
    args: Vec<std::ffi::OsString>,
) -> std::io::Result<()> {
    Command::new(system32(r"WindowsPowerShell\v1.0\powershell.exe"))
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(script(pid, installer, dir, &args))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The installer goes over this install, silently, after the window and its host are gone,
    /// and the window it starts is this install's.
    #[test]
    fn the_installer_runs_over_this_install_then_the_window_starts() {
        let dir = Path::new(r"C:\Users\me\AppData\Local\Programs\Fibo");
        let setup = Path::new(r"C:\Users\me\AppData\Local\Fibo\updates\0.2.0\Fibo-Setup-0.2.0.exe");
        let s = script(42, setup, dir, &["--runtime".into(), r"C:\rt".into()]);
        let wait = s.find("Wait-Process -Id 42").unwrap();
        let stop = s.find("Stop-Process -Force").unwrap();
        let install = s.find("Start-Process -FilePath").unwrap();
        let start = s.find(r"Programs\Fibo\bin\fibo-app.exe").unwrap();
        assert!(wait < stop && stop < install && install < start, "{s}");
        assert!(s.contains(r#"/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR="C:\Users\me\AppData\Local\Programs\Fibo""#), "{s}");
        assert!(
            s.contains(r"$bin = 'C:\Users\me\AppData\Local\Programs\Fibo\bin'"),
            "{s}"
        );
        assert!(s.contains("-Wait;"), "{s}");
        assert!(s.ends_with(r"'--runtime' 'C:\rt'"), "{s}");
    }
}
