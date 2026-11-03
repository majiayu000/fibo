//! Which branch a directory is on, read straight from the repository's HEAD.
use std::path::Path;

/// The branch checked out where `dir` sits, from `.git/HEAD` (a linked worktree's `.git`
/// file points at its own HEAD); a detached HEAD shows its short commit.
pub(super) fn read_branch(dir: &Path) -> Option<String> {
    for d in dir.ancestors() {
        let dot = d.join(".git");
        let git_dir = if dot.is_dir() {
            dot
        } else if dot.is_file() {
            let text = std::fs::read_to_string(&dot).ok()?;
            d.join(text.trim().strip_prefix("gitdir:")?.trim())
        } else {
            continue;
        };
        let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
        let head = head.trim();
        return Some(match head.strip_prefix("ref: refs/heads/") {
            Some(branch) => branch.to_owned(),
            None => head.chars().take(7).collect(),
        });
    }
    None
}
#[cfg(test)]
mod tests {
    #[test]
    fn branch_comes_from_head_in_a_repo_or_a_linked_worktree() {
        let root = std::env::temp_dir().join(format!("kl-branch-{}", std::process::id()));
        let repo = root.join("repo");
        let nested = repo.join("a/b");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/feat/ui-main\n").unwrap();
        assert_eq!(super::read_branch(&nested).as_deref(), Some("feat/ui-main"));
        let linked = root.join("linked");
        let gitdir = repo.join(".git/worktrees/linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::write(gitdir.join("HEAD"), "0123456789abcdef\n").unwrap();
        std::fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .unwrap();
        assert_eq!(super::read_branch(&linked).as_deref(), Some("0123456"));
        assert_eq!(super::read_branch(&root), None);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
