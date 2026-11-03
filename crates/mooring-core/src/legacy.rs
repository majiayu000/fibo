//! Mooring was called Keepline before. An install from then keeps working after the
//! update: terminals started by the old host and the Agents in them still carry
//! `KEEPLINE_*` variables, Claude's settings still hold hook entries that test
//! `$KEEPLINE_HOOK`, the user's plugins may read `KEEPLINE_CWD`, the config lives in
//! `~/.config/keepline` and a host may be serving `~/.local/share/keepline-workbench`.
use std::path::{Path, PathBuf};

const OLD: &str = "KEEPLINE_";
const NEW: &str = "MOORING_";

/// At start, before any other thread: every `KEEPLINE_*` variable is copied to its
/// `MOORING_*` name when that one is not set, so code reading the new names also sees
/// an environment set up under the old ones.
pub fn adopt_old_env() {
    for (key, value) in std::env::vars_os() {
        let Some(rest) = key.to_str().and_then(|k| k.strip_prefix(OLD)) else {
            continue;
        };
        let new = format!("{NEW}{rest}");
        if std::env::var_os(&new).is_none() {
            // SAFETY: called first thing in `main`, before any thread reads the environment.
            unsafe { std::env::set_var(&new, value) };
        }
    }
}

/// The old name of a variable Mooring sets for programs it runs, which gets the same
/// value: an Agent, script or plugin written for Keepline still finds it.
pub fn old_name(var: &str) -> Option<String> {
    var.strip_prefix(NEW).map(|rest| format!("{OLD}{rest}"))
}

/// `vars` with each `MOORING_*` entry also under its old name.
pub fn with_old_names<V: Clone>(vars: Vec<(String, V)>) -> Vec<(String, V)> {
    let old: Vec<(String, V)> = vars
        .iter()
        .filter_map(|(k, v)| old_name(k).map(|o| (o, v.clone())))
        .collect();
    vars.into_iter().chain(old).collect()
}

/// Moves `<base>/keepline` to `<base>/mooring` once, `<base>` being the config base
/// (`$XDG_CONFIG_HOME` or `~/.config`), and leaves a link at the old place for a Keepline
/// app or host still running from before. Nothing happens when the new one exists.
pub fn move_old_config() {
    let Some(new) = crate::agent::config_dir() else {
        return;
    };
    if let Some(base) = new.parent()
        && let Err(e) = move_dir(&base.join("keepline"), &new)
    {
        eprintln!("移动旧配置目录失败：{e}");
    }
}

fn move_dir(old: &Path, new: &Path) -> std::io::Result<()> {
    let Ok(meta) = std::fs::symlink_metadata(old) else {
        return Ok(());
    };
    if new.exists() {
        return Ok(());
    }
    if meta.is_symlink() {
        // The user's own link (dotfiles): Mooring links to the same directory and
        // leaves theirs alone. The link this move leaves behind points at `new`, which
        // exists, so it never comes here.
        return match std::fs::canonicalize(old) {
            Ok(real) if real.is_dir() => std::os::unix::fs::symlink(real, new),
            _ => Ok(()),
        };
    }
    if !meta.is_dir() {
        return Ok(());
    }
    std::fs::rename(old, new)?;
    std::os::unix::fs::symlink(new, old)
}

/// The runtime directory under `home`: the new one, unless only the old one exists. A
/// host from before the rename may be serving it, with the user's terminals.
pub fn runtime_dir(home: &Path) -> PathBuf {
    let new = home.join(".local/share/mooring-workbench");
    let old = home.join(".local/share/keepline-workbench");
    if !new.exists() && old.is_dir() {
        old
    } else {
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_get_both_names() {
        let vars = with_old_names(vec![
            ("MOORING_CWD".to_owned(), "/x"),
            ("PATH".to_owned(), "/bin"),
        ]);
        assert_eq!(
            vars,
            [
                ("MOORING_CWD".to_owned(), "/x"),
                ("PATH".to_owned(), "/bin"),
                ("KEEPLINE_CWD".to_owned(), "/x"),
            ]
        );
    }

    #[test]
    fn the_old_config_moves_once_and_leaves_a_link() {
        let base = std::env::temp_dir().join(format!("mooring-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (old, new) = (base.join("keepline"), base.join("mooring"));
        std::fs::create_dir_all(old.join("plugins/mine")).unwrap();
        move_dir(&old, &new).unwrap();
        assert!(new.join("plugins/mine").is_dir(), "moved");
        assert!(std::fs::symlink_metadata(&old).unwrap().is_symlink());
        assert!(
            old.join("plugins/mine").is_dir(),
            "the old path still reaches it"
        );
        move_dir(&old, &new).unwrap(); // a link, not a directory: nothing to do
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_host_from_before_keeps_its_runtime() {
        let home = std::env::temp_dir().join(format!("mooring-runtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert!(
            runtime_dir(&home).ends_with("mooring-workbench"),
            "a new install"
        );
        std::fs::create_dir_all(home.join(".local/share/keepline-workbench")).unwrap();
        assert!(
            runtime_dir(&home).ends_with("keepline-workbench"),
            "an old one"
        );
        std::fs::create_dir_all(home.join(".local/share/mooring-workbench")).unwrap();
        assert!(runtime_dir(&home).ends_with("mooring-workbench"));
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// Dotfiles often make `~/.config/keepline` a link to a real directory. That is
    /// the user's config: Mooring links to the same directory and keeps their link.
    #[test]
    fn a_linked_old_config_is_adopted_where_it_lives() {
        let base = std::env::temp_dir().join(format!("mooring-legacy-ln-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("dotfiles/keepline");
        std::fs::create_dir_all(real.join("plugins/mine")).unwrap();
        let (old, new) = (base.join("keepline"), base.join("mooring"));
        std::os::unix::fs::symlink(&real, &old).unwrap();
        move_dir(&old, &new).unwrap();
        assert!(new.join("plugins/mine").is_dir(), "adopted");
        assert_eq!(
            std::fs::read_link(&old).unwrap(),
            real,
            "their link is untouched"
        );
        assert!(
            real.join("plugins/mine").is_dir(),
            "the real directory stays put"
        );
        move_dir(&old, &new).unwrap(); // done once
        // A link to nowhere is left alone.
        let (dangling, other) = (base.join("gone"), base.join("other"));
        std::os::unix::fs::symlink(base.join("missing"), &dangling).unwrap();
        move_dir(&dangling, &other).unwrap();
        assert!(!other.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
