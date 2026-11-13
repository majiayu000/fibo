//! The product was called Keepline, then Mooring, and is Fibo now. An install from
//! before keeps working after the update: terminals started by an old host and the
//! Agents in them may carry only `MOORING_*` variables, plugins read `MOORING_CWD`, the
//! config may still be in `~/.config/mooring`, and a host may be serving
//! `~/.local/share/keepline-workbench` or `mooring-workbench`.
//!
//! `MOORING_*` stays the name the code reads; `FIBO_*` is set beside it for programs and
//! adopted at start. Hook entries test `$FIBO_HOOK` first and fall back to `$MOORING_HOOK`.
//! Keepline was never released: its `KEEPLINE_*` names were dropped on 11-13, once no
//! terminal on the one machine that ran it carried them alone. Its runtime directory
//! stays, since that machine's host still serves it.
use std::path::{Path, PathBuf};

const NEW: &str = "MOORING_";
/// The product's name for the same variables, set beside `MOORING_*`.
const BRAND: &str = "FIBO_";

/// At start, before any other thread: every `FIBO_*` variable is copied to its
/// `MOORING_*` name when that one is not set, so code reading `MOORING_*` also sees an
/// environment set up under the product's name.
pub fn adopt_old_env() {
    for (key, value) in std::env::vars_os() {
        let Some(rest) = key.to_str().and_then(|k| k.strip_prefix(BRAND)) else {
            continue;
        };
        let new = format!("{NEW}{rest}");
        if std::env::var_os(&new).is_none() {
            // SAFETY: called first thing in `main`, before any thread reads the environment.
            unsafe { std::env::set_var(&new, value) };
        }
    }
}

/// The product's name of a `MOORING_*` variable Fibo sets for programs, which gets the
/// same value.
pub fn fibo_name(var: &str) -> Option<String> {
    var.strip_prefix(NEW).map(|rest| format!("{BRAND}{rest}"))
}

/// `vars` with each `MOORING_*` entry also under its `FIBO_*` name.
pub fn with_fibo_names<V: Clone>(vars: Vec<(String, V)>) -> Vec<(String, V)> {
    let brand: Vec<(String, V)> = vars
        .iter()
        .filter_map(|(k, v)| fibo_name(k).map(|o| (o, v.clone())))
        .collect();
    vars.into_iter().chain(brand).collect()
}

/// Moves `<base>/mooring` to `<base>/fibo` once,
/// `<base>` being the config base (`$XDG_CONFIG_HOME` or `~/.config`), and leaves a link
/// at the old place for an app or host still running from before. Nothing happens when
/// the new one exists.
pub fn move_old_config() {
    let Some(new) = crate::agent::config_dir() else {
        return;
    };
    if let Some(base) = new.parent()
        && let Err(e) = move_dir(&base.join("mooring"), &new)
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
        // The user's own link (dotfiles): Fibo links to the same directory and
        // leaves theirs alone. The link this move leaves behind points at `new`, which
        // exists, so it never comes here.
        return match std::fs::canonicalize(old) {
            Ok(real) if real.is_dir() => fibo_platform::fs::link_dir(real, new),
            _ => Ok(()),
        };
    }
    if !meta.is_dir() {
        return Ok(());
    }
    std::fs::rename(old, new)?;
    fibo_platform::fs::link_dir(new, old)
}

/// The runtime directory under `home`: the new one, unless only an old one exists
/// (Mooring's before Keepline's). A host from before the rename may be serving it, with
/// the user's terminals.
pub fn runtime_dir(home: &Path) -> PathBuf {
    let share = home.join(".local/share");
    let new = share.join("fibo-workbench");
    if new.exists() {
        return new;
    }
    ["mooring-workbench", "keepline-workbench"]
        .into_iter()
        .map(|old| share.join(old))
        .find(|old| old.is_dir())
        .unwrap_or(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_get_both_names() {
        let vars = with_fibo_names(vec![
            ("MOORING_CWD".to_owned(), "/x"),
            ("PATH".to_owned(), "/bin"),
        ]);
        assert_eq!(
            vars,
            [
                ("MOORING_CWD".to_owned(), "/x"),
                ("PATH".to_owned(), "/bin"),
                ("FIBO_CWD".to_owned(), "/x"),
            ]
        );
    }

    /// `FIBO_*` reaches `MOORING_*`; a set `MOORING_*` (a terminal an older host started)
    /// wins, and `KEEPLINE_*` no longer counts. Runs itself in a child process, since it
    /// changes the environment.
    #[test]
    fn fibo_names_are_adopted_and_mooring_ones_still_count() {
        const NAME: &str = "legacy::tests::fibo_names_are_adopted_and_mooring_ones_still_count";
        if std::env::var_os("LEGACY_ADOPT_CHILD").is_none() {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
                .env("LEGACY_ADOPT_CHILD", "1")
                .env("MOORING_X_SET", "mooring")
                .env("FIBO_X_SET", "fibo")
                .env("FIBO_X_NEW", "fibo")
                .env("KEEPLINE_X_OLD", "keepline")
                .env_remove("MOORING_X_NEW")
                .env_remove("MOORING_X_OLD")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success() && text.contains("1 passed"), "{text}");
            return;
        }
        adopt_old_env();
        let get = |k| std::env::var(k).ok();
        assert_eq!(
            get("MOORING_X_SET").as_deref(),
            Some("mooring"),
            "an old terminal's MOORING_* stays"
        );
        assert_eq!(get("MOORING_X_NEW").as_deref(), Some("fibo"));
        assert_eq!(get("MOORING_X_OLD"), None);
    }

    /// The config moves once, the old path still reaches it, and the next start does nothing.
    #[cfg(unix)]
    #[test]
    fn a_mooring_config_moves_to_fibo_once_and_leaves_a_link() {
        let base = std::env::temp_dir().join(format!("fibo-legacy-fibo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (mooring, fibo) = (base.join("mooring"), base.join("fibo"));
        std::fs::create_dir_all(mooring.join("plugins/mine")).unwrap();
        move_dir(&mooring, &fibo).unwrap();
        assert!(fibo.join("plugins/mine").is_dir(), "moved");
        assert!(!std::fs::symlink_metadata(&fibo).unwrap().is_symlink());
        assert!(std::fs::symlink_metadata(&mooring).unwrap().is_symlink());
        assert!(
            mooring.join("plugins/mine").is_dir(),
            "the old path still reaches it"
        );
        move_dir(&mooring, &fibo).unwrap(); // a link, not a directory: nothing to do
        assert!(fibo.join("plugins/mine").is_dir());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_host_from_before_keeps_its_runtime() {
        let home = std::env::temp_dir().join(format!("fibo-runtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert!(
            runtime_dir(&home).ends_with("fibo-workbench"),
            "a new install"
        );
        std::fs::create_dir_all(home.join(".local/share/keepline-workbench")).unwrap();
        assert!(
            runtime_dir(&home).ends_with("keepline-workbench"),
            "a Keepline one"
        );
        std::fs::create_dir_all(home.join(".local/share/mooring-workbench")).unwrap();
        assert!(
            runtime_dir(&home).ends_with("mooring-workbench"),
            "a Mooring one, whose host holds the user's terminals"
        );
        std::fs::create_dir_all(home.join(".local/share/fibo-workbench")).unwrap();
        assert!(runtime_dir(&home).ends_with("fibo-workbench"));
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// Dotfiles often make `~/.config/mooring` a link to a real directory. That is
    /// the user's config: Fibo links to the same directory and keeps their link.
    #[cfg(unix)]
    #[test]
    fn a_linked_old_config_is_adopted_where_it_lives() {
        let base = std::env::temp_dir().join(format!("fibo-legacy-ln-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("dotfiles/mooring");
        std::fs::create_dir_all(real.join("plugins/mine")).unwrap();
        let (old, new) = (base.join("mooring"), base.join("fibo"));
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
