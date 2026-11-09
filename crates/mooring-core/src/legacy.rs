//! The product was called Keepline, then Mooring, and is Fibo now. An install from
//! before keeps working after the update: terminals started by an old host and the
//! Agents in them still carry `KEEPLINE_*` or only `MOORING_*` variables, Claude's
//! settings still hold hook entries that test `$KEEPLINE_HOOK`, plugins may read
//! `KEEPLINE_CWD`, the config lives in `~/.config/keepline` or `~/.config/mooring`, and
//! a host may be serving `~/.local/share/keepline-workbench` or `mooring-workbench`.
//!
//! `MOORING_*` stays the name the code reads and the one hooks test: a terminal an older
//! host started has only that one, and it survives the host handing over to a new one.
//! `FIBO_*` is set beside it for programs, and adopted at start like the old names.
use std::path::{Path, PathBuf};

const OLD: &str = "KEEPLINE_";
const NEW: &str = "MOORING_";
/// The product's name for the same variables, set beside `MOORING_*`.
const BRAND: &str = "FIBO_";

/// At start, before any other thread: every `FIBO_*` or `KEEPLINE_*` variable is copied
/// to its `MOORING_*` name when that one is not set (`FIBO_*` first), so code reading
/// `MOORING_*` also sees an environment set up under the other names.
pub fn adopt_old_env() {
    for prefix in [BRAND, OLD] {
        for (key, value) in std::env::vars_os() {
            let Some(rest) = key.to_str().and_then(|k| k.strip_prefix(prefix)) else {
                continue;
            };
            let new = format!("{NEW}{rest}");
            if std::env::var_os(&new).is_none() {
                // SAFETY: called first thing in `main`, before any thread reads the environment.
                unsafe { std::env::set_var(&new, value) };
            }
        }
    }
}

/// The old name of a variable Mooring sets for programs it runs, which gets the same
/// value: an Agent, script or plugin written for Keepline still finds it.
pub fn old_name(var: &str) -> Option<String> {
    var.strip_prefix(NEW).map(|rest| format!("{OLD}{rest}"))
}

/// Every other name a `MOORING_*` variable is set under for programs: `KEEPLINE_*` for
/// what was written for Keepline, `FIBO_*` for what is written for Fibo.
pub fn other_names(var: &str) -> Vec<String> {
    var.strip_prefix(NEW)
        .map(|rest| vec![format!("{OLD}{rest}"), format!("{BRAND}{rest}")])
        .unwrap_or_default()
}

/// `vars` with each `MOORING_*` entry also under its other names.
pub fn with_old_names<V: Clone>(vars: Vec<(String, V)>) -> Vec<(String, V)> {
    let old: Vec<(String, V)> = vars
        .iter()
        .flat_map(|(k, v)| other_names(k).into_iter().map(|o| (o, v.clone())))
        .collect();
    vars.into_iter().chain(old).collect()
}

/// Moves `<base>/mooring`, or before it `<base>/keepline`, to `<base>/fibo` once,
/// `<base>` being the config base (`$XDG_CONFIG_HOME` or `~/.config`), and leaves a link
/// at the old place for an app or host still running from before. Nothing happens when
/// the new one exists.
pub fn move_old_config() {
    let Some(new) = crate::agent::config_dir() else {
        return;
    };
    if let Some(base) = new.parent() {
        for old in ["mooring", "keepline"] {
            if let Err(e) = move_dir(&base.join(old), &new) {
                eprintln!("移动旧配置目录失败：{e}");
            }
        }
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
            Ok(real) if real.is_dir() => mooring_platform::fs::link_dir(real, new),
            _ => Ok(()),
        };
    }
    if !meta.is_dir() {
        return Ok(());
    }
    std::fs::rename(old, new)?;
    mooring_platform::fs::link_dir(new, old)
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
                ("FIBO_CWD".to_owned(), "/x"),
            ]
        );
    }

    /// `FIBO_*` and `KEEPLINE_*` both reach `MOORING_*`; a set `MOORING_*` (a terminal
    /// an older host started) wins, and `FIBO_*` wins over `KEEPLINE_*`. Runs itself in a
    /// child process, since it changes the environment.
    #[test]
    fn other_names_are_adopted_and_mooring_ones_still_count() {
        const NAME: &str = "legacy::tests::other_names_are_adopted_and_mooring_ones_still_count";
        if std::env::var_os("LEGACY_ADOPT_CHILD").is_none() {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
                .env("LEGACY_ADOPT_CHILD", "1")
                .env("MOORING_X_SET", "mooring")
                .env("FIBO_X_SET", "fibo")
                .env("FIBO_X_BOTH", "fibo")
                .env("KEEPLINE_X_BOTH", "keepline")
                .env("KEEPLINE_X_OLD", "keepline")
                .env_remove("MOORING_X_BOTH")
                .env_remove("MOORING_X_OLD")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success() && text.contains("1 passed"), "{text}");
            return;
        }
        adopt_old_env();
        let get = |k| std::env::var(k).unwrap();
        assert_eq!(
            get("MOORING_X_SET"),
            "mooring",
            "an old terminal's MOORING_* stays"
        );
        assert_eq!(get("MOORING_X_BOTH"), "fibo");
        assert_eq!(get("MOORING_X_OLD"), "keepline");
    }

    /// A Mooring install that came from Keepline: `keepline` is already a link to
    /// `mooring`. The moves run in `move_old_config`'s order; the config ends up in
    /// `fibo`, and both old paths still reach it.
    #[cfg(unix)]
    #[test]
    fn a_mooring_config_moves_to_fibo_and_both_old_paths_reach_it() {
        let base = std::env::temp_dir().join(format!("mooring-legacy-fibo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (keepline, mooring, fibo) = (
            base.join("keepline"),
            base.join("mooring"),
            base.join("fibo"),
        );
        std::fs::create_dir_all(keepline.join("plugins/mine")).unwrap();
        move_dir(&keepline, &mooring).unwrap();
        for old in [&mooring, &keepline] {
            move_dir(old, &fibo).unwrap();
        }
        assert!(fibo.join("plugins/mine").is_dir(), "moved");
        assert!(!std::fs::symlink_metadata(&fibo).unwrap().is_symlink());
        assert!(std::fs::symlink_metadata(&mooring).unwrap().is_symlink());
        assert!(mooring.join("plugins/mine").is_dir());
        assert!(keepline.join("plugins/mine").is_dir());
        for old in [&mooring, &keepline] {
            move_dir(old, &fibo).unwrap(); // the next start: nothing to do
        }
        assert!(fibo.join("plugins/mine").is_dir());
        std::fs::remove_dir_all(&base).unwrap();
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

    /// Dotfiles often make `~/.config/keepline` a link to a real directory. That is
    /// the user's config: Mooring links to the same directory and keeps their link.
    #[cfg(unix)]
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
