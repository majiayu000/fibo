//! `file`: the text of a Markdown or plain-text file in a conversation's repository, for the
//! phone's preview of a file link in a chat. Full role only, like `diff`'s patches. The
//! boundary is the git repository holding the terminal's folder (`terminal_cwd`), or that
//! folder when it is in none, never one the phone names; the file is
//! where its path leads once links are followed, so a link inside the folder that points
//! out of it (a key, say) is refused like a path that is outside to begin with.
use serde_json::{json, Value};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Read at most this much: escaped as JSON it still fits one protocol line.
const MOST: u64 = 128 * 1024;
const MARKDOWN: &[&str] = &["md", "markdown"];
const PLAIN: &[&str] = &["txt"];

pub(super) fn file(socket: &Path, request: &Value) -> io::Result<Value> {
    let (terminal, path) = (
        super::text(request, "terminal")?,
        super::text(request, "path")?,
    );
    let w = super::held::workspace(socket)?;
    let term = (w.terminals.iter().find(|x| x.id == terminal))
        .ok_or_else(|| io::Error::other(t!("终端不存在")))?;
    let (full, markdown) = resolve(Path::new(&w.terminal_cwd(term)), path)?;
    let (text, truncated) = read(&full)?;
    Ok(json!({
        "path": full.to_string_lossy(),
        "text": text,
        "markdown": markdown,
        "truncated": truncated,
    }))
}

/// The repository holding `cwd` (the nearest folder with a `.git`), else `cwd` itself: an Agent
/// that `cd`s into `mobile/` still links the repository's `docs/plan.md`.
fn boundary(cwd: &Path) -> PathBuf {
    (cwd.ancestors().find(|d| d.join(".git").exists()))
        .unwrap_or(cwd)
        .to_path_buf()
}

/// Where `path` (absolute, `~/…`, or from `cwd`) really is, and whether it is Markdown;
/// refused unless it is a previewable file inside `cwd`'s repository ([`boundary`]).
fn resolve(cwd: &Path, path: &str) -> io::Result<(PathBuf, bool)> {
    let outside = || io::Error::other(t!("这个文件不在会话所在的仓库里"));
    let path = path.trim();
    let named = match Path::new(path).is_relative() && !path.starts_with('~') {
        true => cwd.join(path),
        false => super::dirs::expand_exact(path)?,
    };
    let cwd = std::fs::canonicalize(cwd).map_err(|_| io::Error::other(t!("目录不存在")))?;
    let root = boundary(&cwd);
    let full = std::fs::canonicalize(&named).map_err(|_| io::Error::other(t!("文件不存在")))?;
    if !full.starts_with(&root) {
        return Err(outside());
    }
    let ext = (full.extension().and_then(|e| e.to_str()))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let markdown = MARKDOWN.contains(&ext.as_str());
    if !markdown && !PLAIN.contains(&ext.as_str()) {
        return Err(io::Error::other(t!("只能预览 Markdown 和纯文本文件")));
    }
    if !full.is_file() {
        return Err(io::Error::other(t!("文件不存在")));
    }
    Ok((full, markdown))
}

/// The file's text, at most [`MOST`] of it, and whether there was more.
fn read(full: &Path) -> io::Result<(String, bool)> {
    let unreadable = |e: io::Error| io::Error::other(t!("读不了这个文件：{e}", e));
    let mut bytes = Vec::new();
    std::fs::File::open(full)
        .and_then(|f| f.take(MOST + 1).read_to_end(&mut bytes))
        .map_err(unreadable)?;
    if bytes.contains(&0) {
        return Err(io::Error::other(t!("这个文件不是文本")));
    }
    let truncated = bytes.len() as u64 > MOST;
    bytes.truncate(MOST as usize);
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fibo-file-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("project/docs")).unwrap();
        std::fs::write(dir.join("project/docs/plan.md"), "# Plan\n").unwrap();
        std::fs::write(dir.join("project/notes.txt"), "notes").unwrap();
        std::fs::write(dir.join("project/main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.join("secret.md"), "key").unwrap();
        dir
    }

    #[test]
    fn a_file_outside_the_sessions_project_is_refused() {
        let dir = folder();
        let root = dir.join("project");
        let (full, markdown) = resolve(&root, "docs/plan.md").unwrap();
        assert!(markdown && full.ends_with("project/docs/plan.md"));
        let abs = root.join("notes.txt");
        assert!(!resolve(&root, abs.to_str().unwrap()).unwrap().1);
        // Out of the folder by `..`, by an absolute path, and by a link inside it.
        let out = t!("这个文件不在会话所在的仓库里");
        assert_eq!(resolve(&root, "../secret.md").unwrap_err().to_string(), out);
        let secret = dir.join("secret.md");
        assert_eq!(
            resolve(&root, secret.to_str().unwrap())
                .unwrap_err()
                .to_string(),
            out
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, root.join("docs/link.md")).unwrap();
            assert_eq!(resolve(&root, "docs/link.md").unwrap_err().to_string(), out);
        }
        // In a repository, a terminal in a subfolder still reaches the rest of it, not past it.
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("mobile")).unwrap();
        assert!(resolve(&root.join("mobile"), "../docs/plan.md").is_ok());
        assert_eq!(
            resolve(&root.join("mobile"), "../../secret.md")
                .unwrap_err()
                .to_string(),
            out
        );
        // Only Markdown and plain text, and only files.
        assert!(resolve(&root, "main.rs").is_err());
        assert!(resolve(&root, "missing.md").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_long_file_is_cut_and_a_binary_one_refused() {
        let dir = folder();
        let long = dir.join("project/long.txt");
        std::fs::write(&long, "a".repeat(MOST as usize + 10)).unwrap();
        let (text, cut) = read(&long).unwrap();
        assert!(cut && text.len() == MOST as usize);
        assert_eq!(
            read(&dir.join("project/notes.txt")).unwrap(),
            ("notes".into(), false)
        );
        std::fs::write(&long, b"a\0b").unwrap();
        assert!(read(&long).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
