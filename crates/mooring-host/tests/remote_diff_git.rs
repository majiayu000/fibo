//! `diff` runs git in folders a phone names, whose contents someone else may have written.
//! No repository's own config may make it run a command, and home is never read as a tree.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const NOT_GIT: &str = "这里不是 git 仓库";
const SECRET: &str = "BEGIN-PRIVATE-KEY";

fn diff(cwd: &Path) -> Value {
    json!({ "op": "diff", "cwd": cwd.to_string_lossy() })
}
fn by_terminal(t: &str) -> Value {
    json!({ "op": "diff", "terminal": t })
}
/// Whichever way this machine's own git config leans, the test's git sees none of it: a
/// global `safe.bareRepository=explicit` would hide what is under test.
fn isolated() -> Vec<(&'static str, &'static Path)> {
    vec![
        ("GIT_CONFIG_GLOBAL", Path::new("/dev/null")),
        ("GIT_CONFIG_NOSYSTEM", Path::new("1")),
    ]
}

/// A repository whose tracked tree holds `inner/`, itself a whole git directory with its
/// work tree at `.`, a config that runs `command`, and a file those keys apply to; cloned,
/// so it got here the way a stranger's project does. The file then changes, as work does.
fn clone_with_bare_inside(h: &Host, command: &str) -> PathBuf {
    let source = scratch(h, "source");
    git(&source, &["init", "-q"]);
    let inner = source.join("inner");
    fs::create_dir(&inner).unwrap();
    git(&inner, &["init", "-q", "--bare"]);
    git(&inner, &["config", "core.bare", "false"]);
    git(&inner, &["config", "core.worktree", "."]);
    fs::write(inner.join("payload.txt"), "one\n").unwrap();
    fs::write(
        inner.join(".gitattributes"),
        "payload.txt diff=x filter=x\n",
    )
    .unwrap();
    git(&inner, &["add", "payload.txt", ".gitattributes"]);
    git(&inner, &["commit", "-q", "-m", "inner"]);
    git(&inner, &["config", "diff.x.textconv", command]);
    git(&inner, &["config", "filter.x.clean", command]);
    git(&source, &["add", "--force", "inner"]);
    git(&source, &["commit", "-q", "-m", "outer"]);
    let clone = h.root.join("clone");
    git(
        &h.root,
        &["clone", "-q", &source.to_string_lossy(), "clone"],
    );
    assert!(clone.join("inner/config").is_file(), "the clone carries it");
    fs::write(clone.join("inner/payload.txt"), "two\n").unwrap();
    clone
}

/// Over the whole request, all roles and both ways in: a checked-in git directory is no
/// repository to `diff`, its config runs nothing, and the project around it still reads.
#[test]
fn a_git_directory_checked_into_a_project_runs_nothing() {
    let h = Host::start();
    let marker = h.root.join("ran");
    let command = format!("/usr/bin/touch '{}'; cat", marker.display());
    let clone = clone_with_bare_inside(&h, &command);
    let inner = clone.join("inner");
    h.launch("project", &clone, "sleep 60");
    h.launch("inside", &inner, "sleep 60");
    for role in ["view", "answer", "full"] {
        let mut r = Remote::open(&h, Some(role), &isolated());
        for (asked, how) in [
            (diff(&inner), "by path"),
            (by_terminal("inside"), "by terminal"),
        ] {
            let reply = r.ask(asked);
            assert!(!marker.exists(), "{role} {how} ran its config: {reply}");
            assert_eq!(reply["error"], NOT_GIT, "{role} {how}: {reply}");
        }
        let d = r.ok(diff(&clone));
        assert!(!marker.exists(), "{role}: the project around it ran it");
        assert_eq!(
            fs::canonicalize(d["cwd"].as_str().unwrap()).unwrap(),
            fs::canonicalize(&clone).unwrap()
        );
    }
}

/// A driver in an ordinary repository's config, however it got there, is not asked to turn
/// files into text for a read: the patch is the file's.
#[test]
fn a_textconv_driver_runs_for_no_role() {
    let h = Host::start();
    let repo = scratch(&h, "repo");
    let marker = h.root.join("ran");
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("a.txt"), "one\n").unwrap();
    fs::write(repo.join(".gitattributes"), "a.txt diff=x\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let command = format!("/usr/bin/touch '{}'; cat", marker.display());
    git(&repo, &["config", "diff.x.textconv", &command]);
    fs::write(repo.join("a.txt"), "two\n").unwrap();
    h.launch("work", &repo, "sleep 60");
    for role in ["view", "full"] {
        let mut r = Remote::open(&h, Some(role), &isolated());
        for asked in [diff(&repo), by_terminal("work")] {
            let d = r.ok(asked);
            assert!(!marker.exists(), "{role}: {d}");
            let patch = d["files"][0]["patch"].as_str().unwrap();
            assert!(patch.contains("-one\n+two\n"), "{patch}");
        }
    }
}

/// `~/.git` makes the whole home one work tree, so `~/.ssh/id_rsa` is an untracked file:
/// a lesser role hears what it would if home were no repository, by path or terminal, from
/// home or from a folder inside it. The full role still reads it.
#[test]
fn a_repository_at_home_is_no_repository_to_a_lesser_role() {
    let h = Host::start();
    let home = scratch(&h, "home");
    git(&home, &["init", "-q"]);
    fs::create_dir(home.join(".ssh")).unwrap();
    fs::write(home.join(".ssh/id_rsa"), format!("{SECRET}\n")).unwrap();
    let notes = home.join("notes");
    fs::create_dir(&notes).unwrap();
    h.launch("at-home", &home, "sleep 60");
    h.launch("at-notes", &notes, "sleep 60");
    let mut env: Vec<(&str, &Path)> = isolated();
    env.push(("HOME", home.as_path()));
    for role in ["view", "answer"] {
        let mut r = Remote::open(&h, Some(role), &env);
        for asked in [
            json!({ "op": "diff", "cwd": "~" }),
            json!({ "op": "diff", "cwd": "~/notes" }),
            by_terminal("at-home"),
            by_terminal("at-notes"),
        ] {
            let reply = r.ask(asked.clone());
            assert_eq!(reply["error"], NOT_GIT, "{role} {asked}: {reply}");
            assert!(!reply.to_string().contains(SECRET), "{reply}");
        }
    }
    let mut full = Remote::open(&h, Some("full"), &env);
    for asked in [json!({ "op": "diff", "cwd": "~" }), by_terminal("at-notes")] {
        assert!(full.ok(asked).to_string().contains(SECRET));
    }
}

/// The same for a repository above home; a repository inside home, read from a folder deep
/// in it, still shows all of itself.
#[test]
fn a_repository_above_home_is_refused_and_one_inside_it_reads_whole() {
    let h = Host::start();
    let above = scratch(&h, "above");
    git(&above, &["init", "-q"]);
    let home = above.join("home");
    fs::create_dir(&home).unwrap();
    fs::write(home.join("id_rsa"), format!("{SECRET}\n")).unwrap();
    let project = home.join("project");
    fs::create_dir_all(project.join("deep")).unwrap();
    git(&project, &["init", "-q"]);
    fs::write(project.join("note.txt"), "note\n").unwrap();
    let plain = home.join("plain");
    fs::create_dir(&plain).unwrap();
    h.launch("at-plain", &plain, "sleep 60");
    h.launch("deep", &project.join("deep"), "sleep 60");
    let mut env: Vec<(&str, &Path)> = isolated();
    env.push(("HOME", home.as_path()));
    let mut r = Remote::open(&h, Some("view"), &env);
    for asked in [diff(&plain), by_terminal("at-plain")] {
        let reply = r.ask(asked);
        assert_eq!(reply["error"], NOT_GIT, "{reply}");
        assert!(!reply.to_string().contains(SECRET), "{reply}");
    }
    for asked in [diff(&project.join("deep")), by_terminal("deep")] {
        let d = r.ok(asked);
        assert_eq!(d["files"][0]["path"], "note.txt", "{d}");
    }
}
