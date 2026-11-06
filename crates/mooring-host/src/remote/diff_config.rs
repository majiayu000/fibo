//! What a repository's own config may ask of a read by a role without `dirs`. A folder can
//! arrive with its `.git` (an unpacked archive), and then its config is a stranger's: a
//! filter runs a command, and `core.worktree` aims git at another folder.
//!
//! Git reads the config, not a parser here: `git config --list` applies git's own rules for
//! case, continuation lines, `include`, `includeIf`, `config.worktree` and a linked work
//! tree's common directory, and runs no command doing it. Global and system entries are the
//! user's own, so only `local` and `worktree` scopes count.

/// What `git lfs install --local` writes, with and without `--skip-smudge`. By value, since
/// the name alone is a stranger's to use.
const GIT_LFS: &[(&str, &str)] = &[
    ("filter.lfs.clean", "git-lfs clean -- %f"),
    ("filter.lfs.smudge", "git-lfs smudge -- %f"),
    ("filter.lfs.smudge", "git-lfs smudge --skip -- %f"),
    ("filter.lfs.process", "git-lfs filter-process"),
    ("filter.lfs.process", "git-lfs filter-process --skip"),
    ("filter.lfs.required", "true"),
];

/// Commands git-lfs's clean runs, so a stranger's needs no filter of its own: the machine's
/// global `filter.lfs.*` runs them. Its subsection keeps its case in what git prints.
const LFS_EXTENSION: &str = "lfs.extension.";

/// The command printing what `foreign` reads.
pub(super) const LIST: &[&str] = &["config", "--list", "--show-scope", "--includes", "-z"];

/// Whether `LIST`'s output has the repository asking for a command or another work tree.
pub(super) fn foreign(list: &[u8]) -> bool {
    // `scope NUL key LF value NUL`, with no LF for a key without a value.
    let mut fields = list.split(|b| *b == 0).filter(|f| !f.is_empty());
    while let (Some(scope), Some(entry)) = (fields.next(), fields.next()) {
        if !matches!(scope, b"local" | b"worktree") {
            continue;
        }
        let entry = String::from_utf8_lossy(entry);
        let (key, value) = entry.split_once('\n').unwrap_or((&entry, ""));
        // Git has lowered the section and key, and kept a subsection's case: `FILTER "x"`
        // arrives as `filter.x`, and `filter "LFS"` is not the name git-lfs writes.
        if key == "core.worktree"
            || (key.starts_with("filter.") && !GIT_LFS.contains(&(key, value)))
            || key.to_ascii_lowercase().starts_with(LFS_EXTENSION)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (scope, entry) in entries {
            out.extend_from_slice(format!("{scope}\0{entry}\0").as_bytes());
        }
        out
    }

    #[test]
    fn a_filter_or_a_work_tree_is_foreign_in_the_repositorys_own_scopes_only() {
        for entry in [
            "filter.x.clean\ncmd",
            "filter.x.process\ncmd",
            "filter.x.required\ntrue",
            "filter.x.clean",
            "core.worktree\n/a",
            "filter.lfs.clean\ncmd",
            "filter.lfs.clean\ngit-lfs clean -- %f; cmd",
            "filter.LFS.clean\ngit-lfs clean -- %f",
            "filter.lfs.unknown\ntrue",
            "lfs.extension.x.clean\n./ext.sh",
            "lfs.EXTENSION.x.clean\n./ext.sh",
            "lfs.Extension.x.priority\n0",
        ] {
            for scope in ["local", "worktree"] {
                assert!(foreign(&list(&[(scope, entry)])), "{scope} {entry}");
            }
            for scope in ["global", "system", "command"] {
                assert!(!foreign(&list(&[(scope, entry)])), "{scope} {entry}");
            }
        }
    }

    #[test]
    fn what_git_lfs_writes_and_ordinary_settings_are_not_foreign() {
        let mut ordinary: Vec<String> = GIT_LFS.iter().map(|(k, v)| format!("{k}\n{v}")).collect();
        ordinary.extend(
            [
                "core.bare\nfalse",
                "user.name\nsomeone",
                "remote.origin.url\nssh://a/b",
                "extensions.partialclone\norigin",
                "worktree.x\ny",
                "notfilter.x.clean\ncmd",
                "lfs.url\nhttps://a/b",
                "lfs.extensions\nx",
            ]
            .map(str::to_owned),
        );
        for entry in &ordinary {
            assert!(!foreign(&list(&[("local", entry)])), "{entry}");
        }
        assert!(!foreign(b""));
        // One among many, and a value holding what looks like an entry.
        assert!(foreign(&list(&[
            ("global", "core.worktree\n/a"),
            ("local", "user.name\nfilter.x.clean"),
            ("local", "filter.x.clean\ncmd"),
        ])));
        assert!(!foreign(&list(&[("local", "user.name\nfilter.x.clean")])));
    }
}
