//! macOS: curl, ditto, plutil, codesign and spctl, all shipped with the system.
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const SUPPORTED: bool = true;

pub fn cache_dir() -> Option<PathBuf> {
    crate::dirs::home().map(|h| h.join("Library/Caches/Mooring/updates"))
}

/// Runs `command`; its stderr (else stdout) is the error.
fn run(mut command: Command, what: &str) -> Result<Output, String> {
    let out = command
        .stdin(Stdio::null())
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
    let mut c = Command::new("/usr/bin/curl");
    c.args(["-fsSL", "--proto", protocols, "--proto-redir", protocols])
        .args(["--max-time", &seconds.to_string()])
        .args(["--max-filesize", &max.to_string()])
        .args(["-A", concat!("Mooring/", env!("CARGO_PKG_VERSION"))]);
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

pub fn unzip(zip: &Path, into: &Path) -> Result<(), String> {
    let mut c = Command::new("/usr/bin/ditto");
    c.args(["-x", "-k"]).arg(zip).arg(into);
    run(c, "ditto").map(drop)
}

pub fn info(app: &Path, key: &str) -> Result<String, String> {
    let mut c = Command::new("/usr/bin/plutil");
    c.args(["-extract", key, "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"));
    let out = run(c, "plutil").map_err(|e| format!("读不了 {key}：{e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

pub fn verify_signature(app: &Path, team: &str) -> Result<(), String> {
    // Beyond an intact signature: Apple's Developer ID chain, issued to `team`.
    let requirement = format!(
        "=anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists \
         and certificate leaf[field.1.2.840.113635.100.6.1.13] exists \
         and certificate leaf[subject.OU] = \"{team}\""
    );
    let mut c = Command::new("/usr/bin/codesign");
    c.args(["--verify", "--deep", "--strict"])
        .arg(format!("-R{requirement}"))
        .arg(app);
    run(c, "codesign").map(drop)
}

pub fn team_id(app: &Path) -> Result<String, String> {
    let mut c = Command::new("/usr/bin/codesign");
    c.args(["-d", "--verbose=2"]).arg(app);
    let out = run(c, "codesign")?;
    // What codesign displays goes to stderr.
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .find_map(|l| l.strip_prefix("TeamIdentifier="))
        .map(|t| t.trim().to_owned())
        .ok_or_else(|| "签名里没有 Team ID".into())
}

pub fn assess(app: &Path) -> Result<(), String> {
    let mut c = Command::new("/usr/sbin/spctl");
    c.args(["--assess", "--type", "execute", "-vv"]).arg(app);
    run(c, "spctl").map(drop)
}

pub fn same_owner(a: &Path, b: &Path) -> std::io::Result<bool> {
    Ok(std::fs::metadata(a)?.uid() == std::fs::metadata(b)?.uid())
}

pub fn homebrew_prefixes() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::env::var_os("HOMEBREW_PREFIX")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    for p in ["/opt/homebrew", "/usr/local"] {
        if !out.iter().any(|o| o == Path::new(p)) {
            out.push(p.into());
        }
    }
    out
}
