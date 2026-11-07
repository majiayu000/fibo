//! Which folders `diff` reads by path for a device that may not see the machine's folders
//! (`dirs` is closed to it): those the workbench already names and what lies inside them.
//! Every refusal is one sentence, decided before `read` looks at the folder or runs git,
//! so a refusal never tells whether a path exists or is a repository.
use mooring_core::{model::Workspace, protocol::*};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const REFUSED: &str = "这个目录不在工作台里，这台设备看不了";

/// The folder to read for `cwd`, if this device may read it. No workspace, no permission.
pub(super) fn checked(socket: &Path, cwd: &str) -> io::Result<PathBuf> {
    let refused = || io::Error::other(REFUSED);
    let Ok(Reply::Workspace(w)) = rpc(socket, &Request::Workspace) else {
        return Err(refused());
    };
    let wanted = super::dirs::expand(cwd).map_err(|_| refused())?;
    let home = super::dirs::home().map_err(|_| refused())?;
    allowed(&wanted, &known(&w), &home).ok_or_else(refused)
}

/// Every folder the workbench names. A space without a folder names none.
fn known(w: &Workspace) -> BTreeSet<PathBuf> {
    (w.terminals.iter().map(|t| w.terminal_cwd(t)))
        .chain(w.agent_sessions.iter().map(|s| s.cwd.clone()))
        .chain(w.spaces.iter().map(|s| s.cwd.clone()))
        .filter_map(|dir| super::dirs::expand_exact(&dir).ok())
        .collect()
}

/// Whether `dir` is home or holds it. No home to compare with: assume it does.
pub(super) fn holds_home(dir: &Path) -> bool {
    let home = super::dirs::home().and_then(|home| home.canonicalize());
    home.map_or(true, |home| holds(dir, &home))
}

/// By file identity, not by path: `/System/Volumes/Data/Users/me` is `/Users/me` through a
/// firmlink, and `realpath` leaves it as written. So `dir` holds home when any tail of
/// home's path, joined to `dir`, is home: this covers home itself, each folder above it,
/// and the data volume's root, which is no folder above it in the path.
fn holds(dir: &Path, home: &Path) -> bool {
    let id = |path: &Path| {
        let metadata = fs::metadata(path).ok()?;
        mooring_platform::fs::file_id(path, &metadata).ok()
    };
    let Some(home_id) = id(home) else {
        return true;
    };
    (home.ancestors())
        .filter_map(|above| home.strip_prefix(above).ok())
        .any(|tail| id(&dir.join(tail)) == Some(home_id))
}

/// Compares real paths by component, so `..` and links can't leave a known folder. A known
/// folder holding home passes only as itself: terminals often start there, and "inside it"
/// would be the whole home. A known folder since deleted stays
/// itself too, for `read` to say it is gone; it answers nothing about any other path.
fn allowed(wanted: &Path, known: &BTreeSet<PathBuf>, home: &Path) -> Option<PathBuf> {
    let Ok(real) = wanted.canonicalize() else {
        return known.contains(wanted).then(|| wanted.to_owned());
    };
    let home = home.canonicalize().ok()?;
    (known.iter().filter_map(|dir| dir.canonicalize().ok()))
        .any(|dir| real == dir || (real.starts_with(&dir) && !holds(&dir, &home)))
        .then_some(real)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A tree of its own: `home/`, `home/project/sub`, `home/projectile`, `elsewhere/`.
    struct Tree(PathBuf);
    impl Tree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("mooring-diff-scope-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            for dir in ["home/project/sub", "home/projectile", "elsewhere"] {
                fs::create_dir_all(root.join(dir)).unwrap();
            }
            Self(root.canonicalize().unwrap())
        }
        fn at(&self, path: &str) -> PathBuf {
            self.0.join(path)
        }
        fn home(&self) -> PathBuf {
            self.at("home")
        }
        fn known(&self, dirs: &[&str]) -> BTreeSet<PathBuf> {
            dirs.iter().map(|d| self.at(d)).collect()
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_known_folder_and_what_is_inside_it_pass_by_their_real_path() {
        let t = Tree::new("inside");
        let known = t.known(&["home/project"]);
        for (asked, real) in [
            ("home/project", "home/project"),
            ("home/project/", "home/project"),
            ("home/project/sub", "home/project/sub"),
            ("home/project/sub/..", "home/project"),
        ] {
            assert_eq!(
                allowed(&t.at(asked), &known, &t.home()),
                Some(t.at(real)),
                "{asked}"
            );
        }
    }

    #[test]
    fn dots_links_and_a_shared_name_prefix_do_not_leave_a_known_folder() {
        let t = Tree::new("escape");
        symlink(t.at("elsewhere"), t.at("home/project/out")).unwrap();
        let known = t.known(&["home/project"]);
        for asked in [
            "home/project/../projectile",
            "home/project/../../elsewhere",
            "home/project/out",
            "home/projectile",
            "home",
            "elsewhere",
            "home/project/missing",
        ] {
            assert_eq!(allowed(&t.at(asked), &known, &t.home()), None, "{asked}");
        }
    }

    /// A known folder reached through a link is known at its real path, and a link into it
    /// from outside lands inside it.
    #[test]
    fn a_known_folder_named_through_a_link_passes_at_its_real_path() {
        let t = Tree::new("named");
        symlink(t.at("home/project"), t.at("elsewhere/alias")).unwrap();
        let known = t.known(&["elsewhere/alias"]);
        assert_eq!(
            allowed(&t.at("home/project/sub"), &known, &t.home()),
            Some(t.at("home/project/sub"))
        );
    }

    #[test]
    fn home_and_what_holds_it_pass_only_as_themselves() {
        let t = Tree::new("wide");
        // Home, its parent (`/Users` on this machine), the root, and home behind a link.
        symlink(t.home(), t.at("elsewhere/homelink")).unwrap();
        let wide = [t.home(), t.0.clone(), PathBuf::from("/")];
        for root in wide {
            let known = BTreeSet::from([root.clone()]);
            assert_eq!(
                allowed(&root, &known, &t.home()),
                Some(root.canonicalize().unwrap()),
                "{root:?} itself"
            );
            assert_eq!(
                allowed(&t.at("home/project"), &known, &t.home()),
                None,
                "inside {root:?}"
            );
        }
        let known = t.known(&["elsewhere/homelink"]);
        assert_eq!(allowed(&t.at("home/project"), &known, &t.home()), None);
        // Its own conversation still opens what home would not.
        let known = t.known(&["home", "home/project"]);
        assert_eq!(
            allowed(&t.at("home/project/sub"), &known, &t.home()),
            Some(t.at("home/project/sub"))
        );
        assert_eq!(allowed(&t.at("home/projectile"), &known, &t.home()), None);
    }

    /// A firmlink names home at a path `realpath` keeps, so only identity sees through it.
    /// The temp folder is under `/private`, which the data volume firmlinks on macOS.
    #[test]
    fn home_through_a_firmlink_is_still_home() {
        let t = Tree::new("firmlink");
        let data = Path::new("/System/Volumes/Data");
        let alias = |path: &Path| data.join(path.strip_prefix("/").unwrap());
        let home = t.home();
        let (aliased, project) = (alias(&home), alias(&t.at("home/project")));
        let id = |p: &Path| mooring_platform::fs::file_id(p, &fs::metadata(p).ok()?).ok();
        if id(&aliased).is_none() || id(&aliased) != id(&home) {
            eprintln!("no firmlink for {home:?} here: nothing to check");
            return;
        }
        assert_eq!(
            aliased.canonicalize().unwrap(),
            aliased,
            "realpath keeps it"
        );
        for root in [aliased.clone(), alias(&t.0), data.to_path_buf()] {
            assert!(holds(&root, &home), "{root:?}");
            let known = BTreeSet::from([root.clone()]);
            assert_eq!(allowed(&project, &known, &home), None, "inside {root:?}");
            assert_eq!(allowed(&root, &known, &home), Some(root.clone()));
        }
        // A project named through the firmlink is no wider than itself.
        let known = BTreeSet::from([project.clone()]);
        assert!(!holds(&project, &home));
        assert_eq!(allowed(&project, &known, &home), Some(project.clone()));
    }

    /// `read` says "目录不存在" of a conversation's deleted folder; what was inside it,
    /// and a path nobody named, stay refused.
    #[test]
    fn a_deleted_known_folder_stays_itself_and_nothing_else() {
        let t = Tree::new("deleted");
        let known = t.known(&["home/gone"]);
        assert_eq!(
            allowed(&t.at("home/gone"), &known, &t.home()),
            Some(t.at("home/gone"))
        );
        assert_eq!(allowed(&t.at("home/gone/sub"), &known, &t.home()), None);
        assert_eq!(allowed(&t.at("home/other"), &known, &t.home()), None);
    }

    /// Terminals by `terminal_cwd`, conversations by their folder, spaces by their path; a
    /// space without a folder must not turn into the host's working directory.
    #[test]
    fn the_workbench_names_its_terminals_conversations_and_spaces() {
        let mut w = mooring_core::sample::every_state();
        let names = known(&w);
        for t in &w.terminals {
            assert!(names.contains(Path::new(&w.terminal_cwd(t))), "{}", t.id);
        }
        for s in w.agent_sessions.iter().filter(|s| !s.cwd.is_empty()) {
            assert!(names.contains(Path::new(&s.cwd)), "{}", s.id);
        }
        for s in w.spaces.iter().filter(|s| !s.cwd.is_empty()) {
            assert!(names.contains(Path::new(&s.cwd)), "{}", s.id);
        }
        w.spaces[0].cwd = String::new();
        assert!(known(&w).iter().all(|dir| dir.is_absolute()));
    }

    #[test]
    fn no_workspace_refuses_like_any_other_path() {
        let t = Tree::new("noworkspace");
        let socket = t.at("no-host.sock");
        let e = checked(&socket, &t.at("home/project").to_string_lossy()).unwrap_err();
        assert_eq!(e.to_string(), REFUSED);
    }
}
