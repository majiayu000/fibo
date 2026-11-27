//! This machine's folders for a client elsewhere: a typed path read as this machine reads
//! it, one folder's subfolders listed, and a new folder made. The phone's `dirs` and
//! `mkdir` and another computer's window (`ListDirs`, `MakeDir`) share these.
use std::io;
use std::path::PathBuf;

/// Folders a listing leaves out besides hidden ones.
const SKIPPED: &[&str] = &["node_modules", "target"];
/// A listing stops at this many; a folder with more is narrowed by typing.
const LISTED: usize = 200;

/// `~` and `~/…` from home; anything else must be absolute. Not trimmed: a folder's name
/// may end in a space.
pub fn expand_exact(path: &str) -> io::Result<PathBuf> {
    let home = || fibo_platform::dirs::home().ok_or_else(|| io::Error::other(t!("没有家目录")));
    let full = match path.strip_prefix('~') {
        Some("") => home()?,
        Some(rest) if rest.starts_with('/') => home()?.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    };
    if !full.is_absolute() {
        return Err(io::Error::other(t!("路径要从 / 或 ~ 开始")));
    }
    Ok(full)
}

/// The folders in `path`, by name, hidden ones and build trees left out.
pub fn list(path: &str) -> io::Result<Vec<String>> {
    let dir = expand_exact(path.trim())?;
    let mut found: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && !SKIPPED.contains(&n.as_str()))
        .collect();
    found.sort_by_key(|n| n.to_lowercase());
    found.truncate(LISTED);
    Ok(found
        .into_iter()
        .map(|n| dir.join(n).to_string_lossy().into_owned())
        .collect())
}

/// One new folder in an existing one. A folder already there is what was asked for, so a
/// resend after a lost reply succeeds too.
pub fn make_dir(path: &str) -> io::Result<PathBuf> {
    let path = expand_exact(path.trim())?;
    let parent = (path.parent())
        .filter(|p| p.is_dir())
        .ok_or_else(|| io::Error::other(t!("上一级文件夹不存在")))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.is_empty() || name == "." || name == ".." {
        return Err(io::Error::other(t!("文件夹名不对")));
    }
    match std::fs::create_dir(parent.join(name)) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(io::Error::other(t!("这里已经有同名文件")));
        }
        r => r?,
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_listed_by_name_and_made_once() {
        let base = std::env::temp_dir().join(format!("kl-folders-{}", std::process::id()));
        for d in ["b", "A", ".hidden", "node_modules"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        std::fs::write(base.join("file"), "").unwrap();
        let s = |p: PathBuf| p.to_string_lossy().into_owned();
        let root = s(base.clone());
        assert_eq!(list(&root).unwrap(), [s(base.join("A")), s(base.join("b"))]);
        let new = s(base.join("new"));
        assert_eq!(s(make_dir(&new).unwrap()), new);
        assert!(make_dir(&new).is_ok(), "made twice");
        assert!(make_dir(&s(base.join("file"))).is_err());
        assert!(make_dir(&s(base.join("no/deeper"))).is_err());
        assert!(list("relative").is_err());
        let _ = std::fs::remove_dir_all(base);
    }
}
