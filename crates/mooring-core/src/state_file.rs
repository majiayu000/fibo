//! Reading a JSON state file that is about to be changed and written back. A file that is not
//! there is empty; one that cannot be read or parsed is unknown, and writing over it would
//! lose what it holds with no one told. Readers that only look may still take it as empty.
use serde::de::DeserializeOwned;
use std::io;
use std::path::Path;

/// `T::default()` when the file is not there, an error naming it when it cannot be understood.
pub fn read_for_update<T: DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => {
            return Err(io::Error::new(
                e.kind(),
                t!("读不了 {path}，未改动：{e}", path = path.display(), e),
            ));
        }
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
