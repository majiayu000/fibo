use super::*;

#[test]
fn worktree_cleanup_rechecks_merge_dirty_in_use_ignored_data_and_space_folders() {
    use std::path::Path;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = fibo_core::git::command(dir)
            .args(args)
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim_end().into()
    }

    let h = Host::start();
    h.setup();
    let repo = h.root.canonicalize().unwrap().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    fs::write(repo.join(".gitignore"), "/target/\n/.private/\n").unwrap();
    git(&repo, &["add", ".gitignore"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--no-gpg-sign",
            "-m",
            "fixture",
        ],
    );
    let merged = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["update-ref", "refs/remotes/origin/main", &merged]);
    git(
        &repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    let tree = |name: &str| {
        let path = repo.with_file_name(name);
        git(
            &repo,
            &["worktree", "add", "-b", name, path.to_str().unwrap()],
        );
        path
    };
    let cache = tree("cache");
    let spaced = repo.with_file_name("cache with spaces");
    git(
        &repo,
        &[
            "worktree",
            "move",
            cache.to_str().unwrap(),
            spaced.to_str().unwrap(),
        ],
    );
    let cache = spaced;
    let whole = tree("whole");
    let dirty = tree("dirty");
    let active = tree("active");
    let external = tree("external-process");
    let ignored = tree("ignored");
    let named = tree("named");
    let unmerged = tree("unmerged");
    let cache_target = cache.join("target");
    fs::create_dir(&cache_target).unwrap();
    fs::write(
        cache_target.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    fs::write(cache_target.join("build"), vec![5; 65_536]).unwrap();
    fs::write(dirty.join("untracked"), "keep").unwrap();
    fs::create_dir(ignored.join(".private")).unwrap();
    fs::write(ignored.join(".private/key"), "keep").unwrap();
    git(
        &unmerged,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--no-gpg-sign",
            "--allow-empty",
            "-m",
            "not merged",
        ],
    );

    for (id, path) in [("repo", &repo), ("named", &named)] {
        h.call(Request::Edit {
            request: None,
            edit: Edit::AddSpace(Space {
                id: id.into(),
                name: id.into(),
                cwd: path.to_str().unwrap().into(),
            }),
        })
        .unwrap();
    }
    let mut launch = h.launch("active", "exec /bin/sleep 60");
    if let Request::Launch { cwd, .. } = &mut launch {
        *cwd = active.to_str().unwrap().into();
    }
    h.call(launch).unwrap();

    let outside = Command::new("/bin/sleep")
        .arg("60")
        .current_dir(&external)
        .spawn_tied()
        .unwrap();

    let Reply::ReclaimableWorktrees(rows) = h.call(Request::ScanWorktrees).unwrap() else {
        panic!("missing worktree list")
    };
    let row = |path: &Path| rows.iter().find(|w| Path::new(&w.path) == path);
    assert!(
        row(&active).is_none()
            && row(&external).is_none()
            && row(&dirty).is_none()
            && row(&unmerged).is_none()
    );
    let cache_row = row(&cache).unwrap();
    let du = Command::new("/usr/bin/du")
        .args(["-skx"])
        .arg(&cache_target)
        .output()
        .unwrap();
    let kib = String::from_utf8(du.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert_eq!(cache_row.target_bytes, Some(kib * 1024));
    assert!(cache_row.total_bytes >= kib * 1024);
    assert_eq!(row(&named).unwrap().space.as_deref(), Some("named"));
    assert!(!row(&named).unwrap().can_remove);
    assert!(!row(&ignored).unwrap().can_remove);

    let reclaim = |path: &Path, entire| {
        h.call(Request::ReclaimWorktree {
            path: path.to_str().unwrap().into(),
            head: merged.clone(),
            entire,
        })
    };
    fs::write(cache.join("new work"), "keep").unwrap();
    assert!(reclaim(&cache, false).is_err() && cache_target.exists());
    fs::remove_file(cache.join("new work")).unwrap();
    assert!(reclaim(&ignored, true).is_err() && ignored.join(".private/key").exists());
    assert!(reclaim(&named, true).is_err() && named.exists());
    assert!(reclaim(&active, true).is_err() && active.exists());
    assert!(matches!(reclaim(&cache, false).unwrap(), Reply::Ok));
    assert!(cache.join(".git").is_file() && !cache_target.exists());
    assert!(matches!(reclaim(&whole, true).unwrap(), Reply::Ok));
    assert!(!whole.exists());
    assert!(!git(&repo, &["worktree", "list", "--porcelain"]).contains(whole.to_str().unwrap()));
    // A saved session can still ask to start in the now-missing directory.
    let mut resume = h.launch("old-cwd", "exec /bin/sleep 60");
    if let Request::Launch { cwd, space, .. } = &mut resume {
        *cwd = whole.to_str().unwrap().into();
        *space = "repo".into();
    }
    let Reply::Workspace(resumed) = h.call(resume).unwrap() else {
        panic!("missing resumed workspace")
    };
    let terminal = resumed
        .terminals
        .iter()
        .find(|t| t.id == "old-cwd")
        .unwrap();
    assert_eq!(resumed.terminal_cwd(terminal), repo.to_str().unwrap());
    h.call(Request::Stop {
        id: "old-cwd".into(),
    })
    .unwrap();
    drop(outside);
    h.call(Request::Stop {
        id: "active".into(),
    })
    .unwrap();
}

/// A bare main repository (`git clone --bare` plus worktrees): git refuses to run inside
/// it under `safe.bareRepository=explicit`, so removal must run from the worktree.
#[test]
fn worktree_cleanup_removes_a_worktree_of_a_bare_repository() {
    use std::path::Path;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = fibo_core::git::command(dir)
            .args(args)
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim_end().into()
    }

    let h = Host::start();
    h.setup();
    let root = h.root.canonicalize().unwrap();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init", "-b", "main"]);
    git(
        &source,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--no-gpg-sign",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    let merged = git(&source, &["rev-parse", "HEAD"]);
    let bare = root.join("bare.git");
    git(
        &root,
        &[
            "clone",
            "--bare",
            "-q",
            source.to_str().unwrap(),
            "bare.git",
        ],
    );
    let in_bare = |args: &[&str]| {
        let mut all = vec!["--git-dir", bare.to_str().unwrap()];
        all.extend_from_slice(args);
        git(&root, &all)
    };
    in_bare(&["update-ref", "refs/remotes/origin/main", &merged]);
    in_bare(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ]);
    let tree = root.join("feature");
    in_bare(&["worktree", "add", "-b", "feature", tree.to_str().unwrap()]);
    // The scan finds the repository through a space on another of its worktrees.
    let anchor = root.join("anchor");
    in_bare(&["worktree", "add", "-b", "anchor", anchor.to_str().unwrap()]);
    h.call(Request::Edit {
        request: None,
        edit: Edit::AddSpace(Space {
            id: "anchor".into(),
            name: "anchor".into(),
            cwd: anchor.to_str().unwrap().into(),
        }),
    })
    .unwrap();

    let Reply::ReclaimableWorktrees(rows) = h.call(Request::ScanWorktrees).unwrap() else {
        panic!("missing worktree list")
    };
    let row = rows
        .iter()
        .find(|w| Path::new(&w.path) == tree)
        .unwrap_or_else(|| panic!("bare repository's worktree not listed: {rows:?}"));
    assert!(row.can_remove);
    let reply = h.call(Request::ReclaimWorktree {
        path: tree.to_str().unwrap().into(),
        head: merged.clone(),
        entire: true,
    });
    assert!(matches!(reply, Ok(Reply::Ok)), "{reply:?}");
    assert!(!tree.exists());
    assert!(!in_bare(&["worktree", "list", "--porcelain"]).contains(tree.to_str().unwrap()));
}
