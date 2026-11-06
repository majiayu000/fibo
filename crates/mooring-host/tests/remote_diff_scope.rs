//! What `diff` reads by path for a device without `dirs`: the workbench's folders only.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const REFUSED: &str = "这个目录不在工作台里，这台设备看不了";
const SECRET: &str = "TOP-SECRET";

/// A repository with one untracked file saying `text`.
fn repo(dir: &Path, text: &str) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q"]);
    fs::write(dir.join("note.txt"), format!("{text}\n")).unwrap();
}
fn diff(cwd: &Path) -> Value {
    json!({ "op": "diff", "cwd": cwd.to_string_lossy() })
}
/// A `git` first on PATH that leaves `marker` behind, to see whether git ran at all.
fn marking_git(h: &Host, marker: &Path) -> PathBuf {
    let bin = scratch(h, "marking-bin");
    let git = bin.join("git");
    let script = format!(
        "#!/bin/sh\n/usr/bin/touch '{}'\nexec /usr/bin/git \"$@\"\n",
        marker.display()
    );
    fs::write(&git, script).unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// Both lesser roles: the terminal's folder and inside it read as ever; everything else gets
/// one sentence whatever is there, and git never starts. The full role is untouched.
#[test]
fn a_lesser_role_reads_only_what_the_workbench_names() {
    let h = Host::start();
    let known = scratch(&h, "known");
    repo(&known, "known note");
    fs::create_dir_all(known.join("sub")).unwrap();
    let outside = scratch(&h, "outside");
    repo(&outside, SECRET);
    // Shares `known`'s characters but not its component.
    let prefixed = scratch(&h, "knownish");
    repo(&prefixed, SECRET);
    let plain = scratch(&h, "plain");
    let afile = plain.join("a-file");
    fs::write(&afile, "x").unwrap();
    symlink(&outside, known.join("out")).unwrap();
    h.launch("work", &known, "sleep 60");

    let marker = h.root.join("git-ran");
    let bin = marking_git(&h, &marker);
    let refused: Vec<PathBuf> = vec![
        outside.clone(),          // a repository
        plain.clone(),            // a folder outside git
        afile,                    // a file
        h.root.join("missing"),   // nothing at all
        known.join("missing"),    // nothing, inside a known folder
        known.join("../outside"), // out by dots
        known.join("out"),        // out by a link
        prefixed.clone(),         // `/a/known` must not pass `/a/knownish`
        PathBuf::from("/"),       // above everything
    ];
    for role in ["view", "answer"] {
        let mut r = Remote::open(&h, Some(role), &[("PATH", &bin)]);
        for path in &refused {
            let reply = r.ask(diff(path));
            assert_eq!(reply["error"], REFUSED, "{role} {path:?}: {reply}");
            assert!(!reply.to_string().contains(SECRET), "{reply}");
        }
        // Naming a terminal too does not widen a path.
        let both = json!({ "op": "diff", "cwd": outside.to_string_lossy(), "terminal": "work" });
        assert_eq!(r.error(both), REFUSED, "{role}");
        assert_eq!(r.error(json!({ "op": "diff", "cwd": "~/.ssh" })), REFUSED);
        assert!(!marker.exists(), "{role}: git ran for a refused path");

        let d = r.ok(diff(&known));
        assert!(
            marker.exists(),
            "{role}: this git must be the one that runs"
        );
        fs::remove_file(&marker).unwrap();
        assert_eq!(d["files"][0]["path"], "note.txt", "{d}");
        let d = r.ok(diff(&known.join("sub")));
        assert_eq!(d["files"][0]["path"], "note.txt", "inside a known folder");
        let d = r.ok(json!({ "op": "diff", "terminal": "work" }));
        assert_eq!(d["files"][0]["path"], "note.txt", "by terminal, as ever");
        fs::remove_file(&marker).unwrap();
    }

    // The full role sees the machine, so it reads anything and hears why not.
    let mut full = Remote::open(&h, Some("full"), &[]);
    let d = full.ok(diff(&outside));
    assert!(d.to_string().contains(SECRET), "{d}");
    assert_eq!(full.error(diff(&plain)), "这里不是 git 仓库");
    assert_eq!(full.error(diff(&h.root.join("missing"))), "目录不存在");
}

/// A terminal started in home names home, not everything in it; a project there opens once
/// the workbench names it. Home's own diff still answers, about home.
#[test]
fn a_terminal_in_home_opens_home_and_not_what_is_in_it() {
    let h = Host::start();
    let home = scratch(&h, "home");
    let project = home.join("project");
    repo(&project, SECRET);
    h.launch("at-home", &home, "sleep 60");

    let mut r = Remote::open(&h, Some("view"), &[("HOME", &home)]);
    assert_eq!(r.error(diff(&project)), REFUSED);
    assert_eq!(
        r.error(json!({ "op": "diff", "cwd": "~/project" })),
        REFUSED
    );
    assert_eq!(
        r.error(json!({ "op": "diff", "cwd": "~/missing" })),
        REFUSED
    );
    assert_eq!(
        r.error(json!({ "op": "diff", "cwd": "~" })),
        "这里不是 git 仓库"
    );

    h.launch("in-project", &project, "sleep 60");
    let d = r.ok(json!({ "op": "diff", "cwd": "~/project" }));
    assert_eq!(d["files"][0]["path"], "note.txt", "{d}");
}

/// A conversation with no terminal left: what the phone's history opens by directory. Once
/// its folder is deleted the phone hears that, not that the folder is off limits.
#[test]
fn a_past_conversations_folder_stays_readable_to_a_lesser_role() {
    const ID: &str = "00000000-0000-4000-8000-000000000067";
    let mut old = PathBuf::new();
    let mut gone = PathBuf::new();
    let h = Host::start_over(|root| {
        old = root.join("old-project");
        gone = root.join("deleted-project");
        repo(&old, "old note");
        let logs = root.join("logs/claude/project");
        fs::create_dir_all(&logs).unwrap();
        for (dir, id) in [(&old, ID), (&gone, "00000000-0000-4000-8000-000000000068")] {
            let row = json!({"type":"user","uuid":"u0","cwd":dir,"sessionId":id,
                "entrypoint":"cli","userType":"external",
                "timestamp":mooring_core::content::now(),"message":{"content":"earlier"}});
            fs::write(logs.join(format!("{id}.jsonl")), format!("{row}\n")).unwrap();
        }
    });
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let found = h.workspace().agent_sessions;
        let has = |dir: &Path| found.iter().any(|s| Path::new(&s.cwd) == dir);
        if has(&old) && has(&gone) {
            break;
        }
        assert!(
            Instant::now() < until,
            "the walk never found them: {found:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(h.workspace().terminals.is_empty(), "no terminal names them");

    for role in ["view", "answer"] {
        let mut r = Remote::open(&h, Some(role), &[]);
        let d = r.ok(diff(&old));
        assert_eq!(d["files"][0]["path"], "note.txt", "{d}");
        assert_eq!(r.error(diff(&gone)), "目录不存在");
        assert_eq!(r.error(diff(&gone.join("sub"))), REFUSED);
    }
}
