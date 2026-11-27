//! A missing state file is empty. One that cannot be read or parsed is unknown, and
//! writing over it would lose what it holds.
use serde::de::DeserializeOwned;
use std::io;
use std::path::Path;

/// A genuinely absent file is empty. A dangling link still points to state we cannot read.
pub(crate) fn read_if_present(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e)
            if e.kind() == io::ErrorKind::NotFound
                && matches!(std::fs::symlink_metadata(path), Err(m) if m.kind() == io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(e) => Err(io::Error::new(
            e.kind(),
            t!("读不了 {path}，未改动：{e}", path = path.display(), e),
        )),
    }
}

/// `T::default()` when the file is not there, an error naming it when it cannot be understood.
pub fn read_for_update<T: DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    let Some(bytes) = read_if_present(path)? else {
        return Ok(T::default());
    };
    serde_json::from_slice(&bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            t!("{path} 格式不对，未改动：{e}", path = path.display(), e),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kl-state-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("state.json")
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_link_is_unreadable_state() {
        let file = scratch("dangling");
        let _ = std::fs::remove_file(&file);
        std::os::unix::fs::symlink("missing.json", &file).unwrap();
        let why = read_for_update::<Vec<String>>(&file).unwrap_err();
        assert!(why.to_string().contains("state.json"), "{why}");
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn a_missing_file_is_empty_and_a_bad_one_is_an_error_that_names_it() {
        let file = scratch("kinds");
        let _ = std::fs::remove_file(&file);
        assert!(read_for_update::<Vec<String>>(&file).unwrap().is_empty());
        std::fs::write(&file, r#"["a"]"#).unwrap();
        assert_eq!(read_for_update::<Vec<String>>(&file).unwrap(), ["a"]);
        for bad in ["[\"a\"", "", "{}", "\0\0\0"] {
            std::fs::write(&file, bad).unwrap();
            let why = read_for_update::<Vec<String>>(&file).unwrap_err();
            assert!(why.to_string().contains("state.json"), "{why}");
        }
    }
}
