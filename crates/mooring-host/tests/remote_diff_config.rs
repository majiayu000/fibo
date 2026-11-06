//! A folder can arrive with its own `.git` (an unpacked archive, say). To a role without
//! `dirs`, a repository whose local config would run a command or move its work tree reads
//! as no repository, however the config is hidden; ordinary ones read as ever.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const NOT_GIT: &str = "这里不是 git 仓库";
const SECRET: &str = "BEGIN-PRIVATE-KEY";

fn isolated() -> Vec<(&'static str, &'static Path)> {
    vec![
        ("GIT_CONFIG_GLOBAL", Path::new("/dev/null")),
        ("GIT_CONFIG_NOSYSTEM", Path::new("1")),
    ]
}
fn diff(cwd: &Path) -> Value {
    json!({ "op": "diff", "cwd": cwd.to_string_lossy() })
}
fn by_terminal(t: &str) -> Value {
    json!({ "op": "diff", "terminal": t })
}
fn append(file: &Path, text: &str) {
    let old = fs::read_to_string(file).unwrap_or_default();
    fs::write(file, old + text).unwrap();
}
fn run_marker(marker: &Path) -> String {
    format!("/usr/bin/touch '{}'; cat", marker.display())
}
/// A repository with a committed `a.txt` under the `x` filter and diff driver, then changed.
fn repo_with_change(dir: &Path, attributes: &str) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q"]);
    fs::write(dir.join("a.txt"), "one\n").unwrap();
    fs::write(dir.join(".gitattributes"), format!("a.txt {attributes}\n")).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "first"]);
    fs::write(dir.join("a.txt"), "two\n").unwrap();
}

/// Every place a filter can hide in what an archive brings, by path and by terminal: the
/// reply is the one a plain folder gets, and the command never runs. The full role is
/// unchanged, which also shows these configs are live.
#[test]
fn a_filter_in_a_repositorys_own_config_reads_as_no_repository() {
    let h = Host::start();
    let known = scratch(&h, "known");
    h.launch("known", &known, "sleep 60");
    let marker = h.root.join("ran");
    let cmd = run_marker(&marker);
    let filter = format!("[filter \"x\"]\n\tclean = {cmd}\n");

    let plain = known.join("plain");
    repo_with_change(&plain, "filter=x");
    append(&plain.join(".git/config"), &filter);

    let shouting = known.join("shouting");
    repo_with_change(&shouting, "filter=x");
    append(
        &shouting.join(".git/config"),
        &format!("[FILTER \"x\"]\n\tCLEAN = {cmd}\n"),
    );

    let included = known.join("included");
    repo_with_change(&included, "filter=x");
    fs::write(known.join("extra-config"), &filter).unwrap();
    append(
        &included.join(".git/config"),
        &format!(
            "[include]\n\tpath = {}\n",
            known.join("extra-config").display()
        ),
    );

    let conditional = known.join("conditional");
    repo_with_change(&conditional, "filter=x");
    append(
        &conditional.join(".git/config"),
        &format!(
            "[includeIf \"gitdir:**/conditional/.git\"]\n\tpath = {}\n",
            known.join("extra-config").display()
        ),
    );

    let per_worktree = known.join("per-worktree");
    repo_with_change(&per_worktree, "filter=x");
    git(
        &per_worktree,
        &["config", "core.repositoryformatversion", "1"],
    );
    git(
        &per_worktree,
        &["config", "extensions.worktreeConfig", "true"],
    );
    append(&per_worktree.join(".git/config.worktree"), &filter);

    let main = known.join("main");
    repo_with_change(&main, "filter=x");
    fs::write(main.join("a.txt"), "one\n").unwrap();
    append(&main.join(".git/config"), &filter);
    let linked = known.join("linked");
    git(&main, &["worktree", "add", "-q", linked.to_str().unwrap()]);
    fs::write(linked.join("a.txt"), "two\n").unwrap();

    // The name git-lfs uses, with a command of someone else's.
    let lfs_name = known.join("lfs-name");
    repo_with_change(&lfs_name, "filter=lfs");
    append(
        &lfs_name.join(".git/config"),
        &format!("[filter \"lfs\"]\n\tclean = {cmd}\n"),
    );

    h.launch("in-plain", &plain, "sleep 60");
    let mut r = Remote::open(&h, Some("view"), &isolated());
    for dir in [
        &plain,
        &shouting,
        &included,
        &conditional,
        &per_worktree,
        &linked,
        &lfs_name,
    ] {
        let reply = r.ask(diff(dir));
        assert!(!marker.exists(), "{dir:?} ran its filter: {reply}");
        assert_eq!(reply["error"], NOT_GIT, "{dir:?}: {reply}");
    }
    let reply = r.ask(by_terminal("in-plain"));
    assert!(!marker.exists(), "by terminal ran its filter: {reply}");
    assert_eq!(reply["error"], NOT_GIT, "{reply}");

    let mut full = Remote::open(&h, Some("full"), &isolated());
    full.ok(diff(&plain));
    assert!(
        marker.exists(),
        "the full role reads this repository as ever"
    );
}

/// `core.worktree` aims `~/proj` at `~/.ssh`: git then runs there, inside home's repository,
/// and lists the key as untracked. No role without `dirs` gets that far.
#[test]
fn a_work_tree_moved_by_config_reads_as_no_repository() {
    let h = Host::start();
    let home = scratch(&h, "home");
    git(&home, &["init", "-q"]);
    let ssh = home.join(".ssh");
    fs::create_dir(&ssh).unwrap();
    fs::write(ssh.join("id_rsa"), format!("{SECRET}\n")).unwrap();
    let project = home.join("proj");
    fs::create_dir(&project).unwrap();
    git(&project, &["init", "-q"]);
    git(
        &project,
        &["config", "core.worktree", ssh.to_str().unwrap()],
    );
    h.launch("in-proj", &project, "sleep 60");
    let mut env = isolated();
    env.push(("HOME", home.as_path()));
    let mut r = Remote::open(&h, Some("view"), &env);
    for asked in [
        json!({ "op": "diff", "cwd": "~/proj" }),
        by_terminal("in-proj"),
    ] {
        let reply = r.ask(asked.clone());
        assert!(!reply.to_string().contains(SECRET), "{asked}: {reply}");
        assert_eq!(reply["error"], NOT_GIT, "{asked}: {reply}");
    }
}

/// A blob missing from a partial clone makes git fetch it, by whatever command the config
/// names. A read for a lesser role fetches nothing.
#[test]
fn a_missing_object_fetches_nothing_for_a_lesser_role() {
    let h = Host::start();
    let known = scratch(&h, "known");
    h.launch("known", &known, "sleep 60");
    let (by_ssh, by_uploadpack) = (h.root.join("ran-ssh"), h.root.join("ran-uploadpack"));
    let setup = |name: &str, keys: &[(&str, String)]| {
        let dir = known.join(name);
        repo_with_change(&dir, "text");
        let blob = Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "rev-parse", "HEAD:a.txt"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        let blob = String::from_utf8(blob.stdout).unwrap();
        let (a, b) = blob.trim().split_at(2);
        fs::remove_file(dir.join(".git/objects").join(a).join(b)).unwrap();
        git(&dir, &["config", "core.repositoryformatversion", "1"]);
        git(&dir, &["config", "extensions.partialClone", "origin"]);
        git(&dir, &["config", "remote.origin.promisor", "true"]);
        for (key, value) in keys {
            git(&dir, &["config", key, value]);
        }
        dir
    };
    let ssh = setup(
        "ssh",
        &[
            ("remote.origin.url", "ssh://example.invalid/x".into()),
            (
                "core.sshCommand",
                format!("/usr/bin/touch '{}'; false", by_ssh.display()),
            ),
        ],
    );
    let uploadpack = setup(
        "uploadpack",
        &[
            ("remote.origin.url", known.to_string_lossy().into_owned()),
            (
                "remote.origin.uploadpack",
                format!("/usr/bin/touch '{}'; false", by_uploadpack.display()),
            ),
        ],
    );
    let mut r = Remote::open(&h, Some("view"), &isolated());
    for (dir, marker) in [(&ssh, &by_ssh), (&uploadpack, &by_uploadpack)] {
        let reply = r.ask(diff(dir));
        assert!(!marker.exists(), "{dir:?} fetched: {reply}");
    }
    let mut full = Remote::open(&h, Some("full"), &isolated());
    let _ = full.ask(diff(&ssh));
    assert!(by_ssh.exists(), "the full role still fetches");
}

/// A submodule is another repository with its own config: git looks inside it for changes
/// only if asked, and a lesser role's read does not ask. Its commit moving still shows.
#[test]
fn a_submodules_own_config_runs_nothing_for_a_lesser_role() {
    let h = Host::start();
    let source = scratch(&h, "submodule-source");
    repo_with_change(&source, "filter=x diff=x");
    fs::write(source.join("a.txt"), "one\n").unwrap();
    let outer = scratch(&h, "outer");
    git(&outer, &["init", "-q"]);
    git(
        &outer,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "--quiet",
            "add",
            source.to_str().unwrap(),
            "sub",
        ],
    );
    git(&outer, &["commit", "-q", "-m", "with sub"]);
    let sub = outer.join("sub");
    let marker = h.root.join("ran");
    let cmd = run_marker(&marker);
    git(&sub, &["config", "filter.x.clean", &cmd]);
    git(&sub, &["config", "diff.x.textconv", &cmd]);
    fs::write(sub.join("a.txt"), "two\n").unwrap();
    h.launch("outer", &outer, "sleep 60");

    let mut r = Remote::open(&h, Some("view"), &isolated());
    let mut full = Remote::open(&h, Some("full"), &isolated());
    for asked in [diff(&outer), by_terminal("outer")] {
        let d = r.ok(asked);
        assert!(!marker.exists(), "the submodule's filter ran: {d}");
    }
    full.ok(diff(&outer));
    assert!(marker.exists(), "the full role still looks inside");
    fs::remove_file(&marker).unwrap();

    // A new commit in it, with the outer repository asking for its inner diff.
    git(&sub, &["add", "a.txt"]);
    git(&sub, &["commit", "-q", "--no-verify", "-m", "moved"]);
    let _ = fs::remove_file(&marker);
    git(&outer, &["config", "diff.submodule", "diff"]);
    let d = r.ok(diff(&outer));
    assert!(!marker.exists(), "the submodule's textconv ran: {d}");
    assert_eq!(d["files"][0]["path"], "sub", "{d}");
    full.ok(diff(&outer));
    assert!(
        marker.exists(),
        "the full role still asks for its inner diff"
    );
}

/// What must keep working: a plain repository, one with an unrelated local setting, a
/// partial clone with its objects, and git-lfs installed for the repository alone.
#[test]
fn ordinary_and_git_lfs_repositories_read_as_ever_for_a_lesser_role() {
    let h = Host::start();
    let known = scratch(&h, "known");
    h.launch("known", &known, "sleep 60");

    let plain = known.join("plain");
    repo_with_change(&plain, "text");
    git(&plain, &["config", "user.name", "someone"]);
    git(
        &plain,
        &["config", "remote.origin.url", "git@example.com:a/b.git"],
    );

    let partial = known.join("partial");
    repo_with_change(&partial, "text");
    git(&partial, &["config", "core.repositoryformatversion", "1"]);
    git(&partial, &["config", "extensions.partialClone", "origin"]);
    git(&partial, &["config", "remote.origin.promisor", "true"]);
    git(
        &partial,
        &["config", "remote.origin.url", "ssh://example.invalid/x"],
    );

    // What `git lfs install --local` writes, less `process`, whose protocol a stand-in does
    // not speak; its value is checked in `diff_config`.
    let lfs = known.join("lfs");
    repo_with_change(&lfs, "filter=lfs");
    git(&lfs, &["config", "filter.lfs.clean", "git-lfs clean -- %f"]);
    git(
        &lfs,
        &["config", "filter.lfs.smudge", "git-lfs smudge -- %f"],
    );
    git(&lfs, &["config", "filter.lfs.required", "true"]);
    let bin = scratch(&h, "lfs-bin");
    let stand_in = bin.join("git-lfs");
    fs::write(&stand_in, "#!/bin/sh\nexec /bin/cat\n").unwrap();
    fs::set_permissions(&stand_in, fs::Permissions::from_mode(0o755)).unwrap();
    let path = PathBuf::from(format!("{}:/usr/bin:/bin", bin.display()));

    let mut env = isolated();
    env.push(("PATH", path.as_path()));
    for role in ["view", "answer"] {
        let mut r = Remote::open(&h, Some(role), &env);
        for dir in [&plain, &partial, &lfs] {
            let d = r.ok(diff(dir));
            assert_eq!(d["files"][0]["path"], "a.txt", "{dir:?}: {d}");
            let patch = d["files"][0]["patch"].as_str().unwrap();
            assert!(patch.contains("-one\n+two\n"), "{dir:?}: {patch}");
        }
    }
}

/// A `git-lfs` that does what the real one does with an extension: runs its `clean` command
/// from the repository, then passes the file through. CI need not have git-lfs installed.
fn git_lfs_stand_in(h: &Host) -> PathBuf {
    let bin = scratch(h, "lfs-extension-bin");
    let stand_in = bin.join("git-lfs");
    let script = "#!/bin/sh\n\
        ext=$(/usr/bin/git config --get-regexp '^lfs\\.extension\\..*\\.clean$' | /usr/bin/cut -d' ' -f2-)\n\
        [ -n \"$ext\" ] && /bin/sh -c \"$ext\" </dev/null\n\
        exec /bin/cat\n";
    fs::write(&stand_in, script).unwrap();
    fs::set_permissions(&stand_in, fs::Permissions::from_mode(0o755)).unwrap();
    PathBuf::from(format!("{}:/usr/bin:/bin", bin.display()))
}

/// git-lfs's clean runs `lfs.extension.<name>.clean`, so a stranger's config needs no filter
/// of its own: not even `filter.lfs.*` where the machine's global config already has it.
#[test]
fn a_git_lfs_extension_in_a_repositorys_own_config_reads_as_no_repository() {
    let h = Host::start();
    let known = scratch(&h, "known");
    h.launch("known", &known, "sleep 60");
    let marker = h.root.join("ran");
    let script = format!("#!/bin/sh\n/usr/bin/touch '{}'\n", marker.display());
    let extension = |dir: &Path, section: &str| {
        repo_with_change(dir, "filter=lfs");
        fs::write(dir.join("ext.sh"), &script).unwrap();
        fs::set_permissions(dir.join("ext.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        append(
            &dir.join(".git/config"),
            &format!("[{section}]\n\tclean = ./ext.sh\n"),
        );
    };
    let lfs_filter = "[filter \"lfs\"]\n\tclean = git-lfs clean -- %f\n\tsmudge = git-lfs smudge -- %f\n\trequired = true\n";

    let with_filter = known.join("with-filter");
    extension(&with_filter, "lfs \"extension.x\"");
    append(&with_filter.join(".git/config"), lfs_filter);
    let no_filter = known.join("no-filter");
    extension(&no_filter, "lfs \"extension.x\"");
    let shouting = known.join("shouting");
    extension(&shouting, "LFS \"EXTENSION.x\"");

    let global = h.root.join("global-gitconfig");
    fs::write(&global, lfs_filter).unwrap();
    let path = git_lfs_stand_in(&h);
    let env: Vec<(&str, &Path)> = vec![
        ("GIT_CONFIG_GLOBAL", global.as_path()),
        ("GIT_CONFIG_NOSYSTEM", Path::new("1")),
        ("PATH", path.as_path()),
    ];
    h.launch("in-no-filter", &no_filter, "sleep 60");
    let mut r = Remote::open(&h, Some("view"), &env);
    for asked in [
        diff(&with_filter),
        diff(&no_filter),
        diff(&shouting),
        by_terminal("in-no-filter"),
    ] {
        let reply = r.ask(asked.clone());
        assert!(!marker.exists(), "{asked} ran its extension: {reply}");
        assert_eq!(reply["error"], NOT_GIT, "{asked}: {reply}");
    }
    let mut full = Remote::open(&h, Some("full"), &env);
    full.ok(diff(&no_filter));
    assert!(
        marker.exists(),
        "the full role reads this repository as ever"
    );
}

/// `foo ` beside `foo`, as an archive can bring them: the name ends in a space, and git must
/// run in that folder and no other. Whoever asks reads `foo `, and `foo`'s filter stays idle.
#[test]
fn a_folder_name_ending_in_a_space_is_read_as_itself() {
    let h = Host::start();
    let known = scratch(&h, "known");
    h.launch("known", &known, "sleep 60");
    let marker = h.root.join("ran");
    let evil = known.join("foo");
    repo_with_change(&evil, "filter=x");
    append(
        &evil.join(".git/config"),
        &format!("[filter \"x\"]\n\tclean = {}\n", run_marker(&marker)),
    );
    let twin = known.join("foo ");
    repo_with_change(&twin, "text");
    let sub = twin.join("sub");
    fs::create_dir(&sub).unwrap();
    h.launch("in-twin", &twin, "sleep 60");
    let top = fs::canonicalize(&twin).unwrap();
    for role in ["view", "full"] {
        let mut r = Remote::open(&h, Some(role), &isolated());
        for asked in [diff(&sub), by_terminal("in-twin")] {
            let reply = r.ask(asked.clone());
            assert!(!marker.exists(), "{role} {asked} ran foo's filter: {reply}");
            assert_eq!(
                reply["ok"]["cwd"].as_str().map(Path::new),
                Some(top.as_path()),
                "{role} {asked}: {reply}"
            );
            assert_eq!(reply["ok"]["files"][0]["path"], "a.txt", "{reply}");
        }
    }
}
