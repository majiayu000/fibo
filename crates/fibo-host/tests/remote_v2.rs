//! The phone app v2's additions to the remote protocol (docs/phone-app/v2-plan.md,
//! 「新增的协议」), each through `fibo remote --stdio` against a host of its own:
//! hello, diff, dirs and mkdir, open with an Agent, quota. Agent screens and logs
//! (answer always, stop, chat tools) are in `remote_v2_agents.rs`.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::model::*;
use serde_json::json;
use std::fs;
use std::process::Command;
use std::time::Duration;

#[test]
fn remote_hello_names_the_os_and_build_and_v2_ops_follow_the_role() {
    let h = Host::start();
    let full = Remote::open(&h, None, &[]);
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    assert_eq!(full.hello["ok"]["os"], os);
    // What Settings shows for this build, read from the window's own crate.
    let app = include_str!("../../fibo-app/Cargo.toml");
    let shown = app
        .lines()
        .find_map(|l| l.strip_prefix("version = "))
        .unwrap();
    assert_eq!(full.hello["ok"]["app_version"], shown.trim_matches('"'));
    assert_eq!(
        full.hello["ok"]["version"], 1,
        "the protocol's version stays"
    );
    for op in ["diff", "dirs", "mkdir", "stop", "quota"] {
        assert!(full.ops().iter().any(|o| o == op), "{op}");
    }
    let view = Remote::open(&h, Some("view"), &[]);
    assert_eq!(view.hello["ok"]["role"], "view");
    let ops = view.ops();
    assert!(ops.contains(&"diff".into()) && ops.contains(&"quota".into()));
    for op in ["dirs", "mkdir", "stop", "open", "answer"] {
        assert!(!ops.iter().any(|o| o == op), "{op} for view: {ops:?}");
    }
    let answer = Remote::open(&h, Some("answer"), &[]);
    assert!(answer.ops().contains(&"answer".into()) && !answer.ops().contains(&"stop".into()));
}

#[test]
fn remote_diff_reads_changes_and_untracked_files_and_caps_patches() {
    let h = Host::start();
    let repo = scratch(&h, "repo");
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(repo.join("gone.txt"), "bye\n").unwrap();
    // Ten files that each change by ~70 KiB: one patch is cut at 64 KiB, and together
    // they pass the 512 KiB budget.
    for i in 0..10 {
        fs::write(repo.join(format!("big{i}.txt")), "old\n").unwrap();
    }
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    fs::write(repo.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
    fs::remove_file(repo.join("gone.txt")).unwrap();
    for i in 0..10 {
        fs::write(
            repo.join(format!("big{i}.txt")),
            "x".repeat(99).add_lines(700),
        )
        .unwrap();
    }
    fs::write(repo.join("new.txt"), "fresh\nfile\n").unwrap();
    h.launch("in-repo", &repo, "sleep 60");

    // A lesser role gets no git run here: no conversation in this terminal, nothing listed.
    let mut view = Remote::open(&h, Some("view"), &[]);
    let d = view.ok(json!({ "op": "diff", "terminal": "in-repo" }));
    assert_eq!(
        (&d["summary"], &d["files"]),
        (&json!(true), &json!([])),
        "{d}"
    );
    // The full role reads the work tree.
    let mut r = Remote::open(&h, Some("full"), &[]);
    let d = r.ok(json!({ "op": "diff", "terminal": "in-repo" }));
    assert!(d.get("summary").is_none(), "{d}");
    let top = fs::canonicalize(&repo).unwrap();
    assert_eq!(fs::canonicalize(d["cwd"].as_str().unwrap()).unwrap(), top);
    assert_eq!(d["branch"], "main");
    let files = d["files"].as_array().unwrap();
    let file = |p: &str| {
        files
            .iter()
            .find(|f| f["path"] == p)
            .unwrap_or_else(|| panic!("{p} not in {files:?}"))
    };
    let a = file("a.txt");
    assert_eq!(
        (
            a["status"].as_str(),
            a["added"].as_u64(),
            a["removed"].as_u64()
        ),
        (Some("M"), Some(2), Some(1))
    );
    let patch = a["patch"].as_str().unwrap();
    assert!(
        patch.starts_with("@@") && patch.contains("-two\n+2\n"),
        "{patch}"
    );
    assert_eq!(a["truncated"], false);
    assert_eq!(file("gone.txt")["status"], "D");
    let new = file("new.txt");
    assert_eq!(
        (new["status"].as_str(), new["added"].as_u64()),
        (Some("?"), Some(2))
    );
    assert!(new["patch"].as_str().unwrap().contains("+fresh\n+file\n"));
    let big = file("big0.txt");
    assert_eq!(big["added"], 700, "counts stay whole when the patch is cut");
    assert_eq!(big["truncated"], true);
    assert!(big["patch"].as_str().unwrap().len() <= 64 * 1024);
    let total: usize = files
        .iter()
        .map(|f| f["patch"].as_str().unwrap().len())
        .sum();
    assert!(total <= 512 * 1024, "{total}");
    assert_eq!(d["truncated"], true);
    assert!(files
        .iter()
        .filter(|f| f["path"].as_str().unwrap().starts_with("big"))
        .any(|f| f["patch"] == ""));
    assert_eq!(
        d["added"].as_u64().unwrap(),
        files
            .iter()
            .map(|f| f["added"].as_u64().unwrap())
            .sum::<u64>()
    );

    // By directory too; a folder outside git is refused in the user's words. The view role
    // reads the terminal's own; the full role, folders nothing here names.
    let by_cwd = r.ok(json!({ "op": "diff", "cwd": repo.to_string_lossy() }));
    assert_eq!(by_cwd["files"].as_array().unwrap().len(), files.len());
    let mut r = Remote::open(&h, Some("full"), &[]);
    let plain = scratch(&h, "plain");
    let e = r.error(json!({ "op": "diff", "cwd": plain.to_string_lossy() }));
    assert_eq!(e, "这里不是 git 仓库");
    // A rename, and a link pointing out of the tree: listed, its target never shown.
    let moved = scratch(&h, "moved");
    git(&moved, &["init", "-q"]);
    let body: String = (0..20).map(|i| format!("line {i}\n")).collect();
    fs::write(moved.join("old name.txt"), &body).unwrap();
    git(&moved, &["add", "."]);
    git(&moved, &["commit", "-q", "-m", "first"]);
    git(&moved, &["mv", "old name.txt", "new name.txt"]);
    fs::write(moved.join("new name.txt"), body + "one more\n").unwrap();
    fs::write(moved.join("after.txt"), "x\n").unwrap();
    let secret = plain.join("secret");
    fs::write(&secret, "TOP-SECRET\n").unwrap();
    std::os::unix::fs::symlink(&secret, moved.join("link")).unwrap();
    let d = r.ok(json!({ "op": "diff", "cwd": moved.to_string_lossy() }));
    let files = d["files"].as_array().unwrap();
    let renamed = files.iter().find(|f| f["status"] == "R").unwrap();
    assert_eq!(renamed["path"], "new name.txt", "{d}");
    assert_eq!(
        (renamed["added"].as_u64(), renamed["removed"].as_u64()),
        (Some(1), Some(0))
    );
    assert!(renamed["patch"].as_str().unwrap().contains("+one more"));
    let link = files.iter().find(|f| f["path"] == "link").unwrap();
    assert_eq!(link["status"], "?");
    assert!(!d.to_string().contains("TOP-SECRET"), "{d}");
    // No commit yet: everything is new, and it is still a repository.
    let unborn = scratch(&h, "unborn");
    git(&unborn, &["init", "-q"]);
    fs::write(unborn.join("first.txt"), "a\n").unwrap();
    let d = r.ok(json!({ "op": "diff", "cwd": unborn.to_string_lossy() }));
    assert_eq!(d["branch"], "main");
    assert_eq!(d["files"][0]["status"], "?");
    // Control characters grow sixfold in JSON: the reply still fits one protocol line.
    let noisy = scratch(&h, "noisy");
    git(&noisy, &["init", "-q"]);
    let line = format!("{}\n", "\u{1}".repeat(99));
    for i in 0..12 {
        fs::write(noisy.join(format!("n{i:02}.txt")), line.repeat(600)).unwrap();
    }
    let reply = r.ask(json!({ "op": "diff", "cwd": noisy.to_string_lossy() }));
    assert!(reply.to_string().len() <= fibo_remote::protocol::MAX_LINE);
    assert_eq!(reply["ok"]["truncated"], true);
    assert_eq!(reply["ok"]["files"].as_array().unwrap().len(), 12);
    assert!(
        reply["ok"]["added"].as_u64().unwrap() == 7200,
        "counts stay"
    );
    assert!(reply["ok"]["files"][0]["patch"].as_str().unwrap().len() > 1000);

    // Read only: git left no lock and no fsmonitor behind.
    assert!(!repo.join(".git/index.lock").exists());
}

#[test]
fn remote_dirs_lists_finds_and_makes_folders_for_the_full_role_only() {
    let h = Host::start();
    let home = scratch(&h, "home");
    let proj = home.join("proj");
    for d in [
        "proj",
        ".hidden",
        "node_modules",
        "target",
        "Notes",
        "deep/a/b/match-me",
        "deep/a/b/c/match-five",
        "deep/a/b/c/d/match-six",
        "deep/a/b/c/d/e/match-too-deep",
        "Desktop/code/AI/tools/mooring",
        "Library/match-in-library",
    ] {
        fs::create_dir_all(home.join(d)).unwrap();
    }
    git(&proj, &["init", "-q"]);
    fs::write(proj.join("f.txt"), "x").unwrap();
    git(&proj, &["add", "."]);
    git(&proj, &["commit", "-q", "-m", "first"]);
    git(
        &proj,
        &["worktree", "add", "-q", "-b", "side", "../proj-wt"],
    );
    fs::write(proj.join("untracked.txt"), "y").unwrap();
    fs::write(proj.join("f.txt"), "changed").unwrap();
    h.launch("in-notes", &home.join("Notes"), "sleep 60");

    let env = [("HOME", home.as_path())];
    let mut r = Remote::open(&h, None, &env);
    let l = r.ok(json!({ "op": "dirs" }));
    assert_eq!(l["path"], home.to_string_lossy().as_ref());
    assert_eq!(l["home"], home.to_string_lossy().as_ref());
    assert_eq!(l["parent"], h.root.to_string_lossy().as_ref());
    let names: Vec<&str> = l["dirs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    // Where a terminal works first, then repositories, then by name; hidden and build trees left out.
    assert_eq!(
        names,
        ["Notes", "proj", "proj-wt", "deep", "Desktop", "Library"],
        "{l}"
    );
    let dir = |name: &str| {
        l["dirs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(
        dir("proj")["git"],
        json!({ "branch": "main", "changes": 2, "worktree": false })
    );
    assert_eq!(dir("proj-wt")["git"]["worktree"], true);
    assert_eq!(dir("proj-wt")["git"]["branch"], "side");
    assert_eq!(dir("Notes")["recent"], true);
    assert!(dir("Notes").get("git").is_none());
    assert!(l["recent"]
        .as_array()
        .unwrap()
        .contains(&json!(home.join("Notes").to_string_lossy())));
    // `~` expands; a path inside lists its own folders.
    let inside = r.ok(json!({ "op": "dirs", "path": "~/deep" }));
    assert_eq!(inside["path"], home.join("deep").to_string_lossy().as_ref());
    assert_eq!(inside["dirs"][0]["name"], "a");

    let found = r.ok(json!({ "op": "dirs", "query": "MATCH" }));
    let paths: Vec<&str> = (found["dirs"].as_array().unwrap().iter())
        .map(|d| d["path"].as_str().unwrap())
        .collect();
    let wanted = [
        "deep/a/b/c/match-five",
        "deep/a/b/match-me",
        "deep/a/b/c/d/match-six",
    ];
    assert_eq!(
        paths,
        wanted.map(|p| home.join(p).to_string_lossy().into_owned()),
        "by name, six levels at most, Library skipped"
    );
    assert_eq!(found["truncated"], false);
    assert_eq!(found["path"], "");
    // Five levels down, where this project's user keeps the folder they open most.
    let found = r.ok(json!({ "op": "dirs", "query": "mooring" }));
    assert_eq!(found["dirs"].as_array().unwrap().len(), 1, "{found}");
    assert_eq!(
        found["dirs"][0]["path"],
        home.join("Desktop/code/AI/tools/mooring")
            .to_string_lossy()
            .as_ref()
    );
    let e = r.error(json!({ "op": "dirs", "path": "relative/path" }));
    assert!(e.contains("/ 或 ~"), "{e}");

    let made = r.ok(json!({ "op": "mkdir", "path": "~/proj/new-one", "request": "m1" }));
    assert_eq!(
        made["path"],
        proj.join("new-one").to_string_lossy().as_ref()
    );
    assert!(proj.join("new-one").is_dir());
    // Sent again after a lost reply: the folder is there, as asked.
    r.ok(json!({ "op": "mkdir", "path": "~/proj/new-one", "request": "m1" }));
    let e = r.error(json!({ "op": "mkdir", "path": "~/nowhere/new", "request": "m2" }));
    assert!(e.contains("上一级文件夹不存在"), "{e}");
    let e = r.error(json!({ "op": "mkdir", "path": "~/proj/f.txt", "request": "m3" }));
    assert!(e.contains("同名文件"), "{e}");

    // A lesser role sees no folders and makes none.
    for role in ["view", "answer"] {
        let mut lesser = Remote::open(&h, Some(role), &env);
        let e = lesser.error(json!({ "op": "dirs" }));
        assert!(e.contains("不能 dirs"), "{e}");
        let e = lesser.error(json!({ "op": "mkdir", "path": "~/x", "request": "m9" }));
        assert!(e.contains("不能 mkdir"), "{e}");
    }
    assert!(!home.join("x").exists());
}

#[test]
fn remote_open_runs_an_agent_with_its_first_message_as_one_argument() {
    let h = Host::start();
    let bin = scratch(&h, "bin");
    let said = h.root.join("said.txt");
    let pwned = h.root.join("pwned");
    // A Claude that writes what it was given and waits; a shell that finds it on PATH
    // (a real login shell's profile could put an installed Claude first).
    let claude = bin.join("claude");
    fs::write(
        &claude,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$#\" \"$@\" >> {}\necho FAKE-CLAUDE-READY\nexec sleep 60\n",
            shell_quote(&said.to_string_lossy())
        ),
    )
    .unwrap();
    let shell = bin.join("fake-shell");
    fs::write(
        &shell,
        format!(
            "#!/bin/sh\nwhile [ \"$1\" = -l ] || [ \"$1\" = -i ]; do shift; done\nPATH={}:$PATH exec /bin/sh \"$@\"\n",
            shell_quote(&bin.to_string_lossy())
        ),
    )
    .unwrap();
    for f in [&claude, &shell] {
        Command::new("chmod").arg("+x").arg(f).status().unwrap();
    }
    let mut r = Remote::open(&h, None, &[("SHELL", shell.as_path())]);
    let prompt = format!(
        "it's \"quoted\" $(touch {}) `id`; echo hi\nsecond line",
        pwned.display()
    );
    let open = json!({ "op": "open", "cwd": "/tmp", "agent": "claude", "prompt": prompt, "request": "o1" });
    let first = r.ok(open.clone());
    let terminal = first["terminal"].as_str().unwrap().to_owned();
    assert!(first["tab"].is_string());
    h.wait_screen(&terminal, "FAKE-CLAUDE-READY");
    let again = r.ok(open);
    assert_eq!(again, first, "a resend opens nothing new");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        fs::read_to_string(&said).unwrap(),
        format!("1\n{prompt}\n"),
        "one argument, verbatim, given once"
    );
    assert!(!pwned.exists(), "nothing in the prompt ran");
    let tabs = h.workspace().tabs;
    assert!(tabs
        .iter()
        .any(|t| t.layout.as_ref().is_some_and(|l| l.contains(&terminal))));

    let e = r.error(json!({ "op": "open", "agent": "nope", "request": "o2" }));
    assert!(e.contains("不认识的 agent nope"), "{e}");
    let e = r.error(json!({ "op": "open", "prompt": "hi", "request": "o3" }));
    assert!(e.contains("和 agent 一起"), "{e}");
    let mut view = Remote::open(&h, Some("view"), &[]);
    assert!(view
        .error(json!({ "op": "open", "agent": "claude", "request": "o4" }))
        .contains("不能 open"));
}

#[test]
fn remote_quota_reports_each_agents_reading_from_the_last_week() {
    let h = Host::start();
    let config = scratch(&h, "config");
    let mut r = Remote::open(&h, Some("view"), &[("XDG_CONFIG_HOME", config.as_path())]);
    assert_eq!(r.ok(json!({ "op": "quota" })), json!({ "agents": [] }));
    drop(r);
    // A quota plugin reading a file, as `claude-quota` reads Claude Code's status line.
    let plugin = config.join("fibo/plugins/q");
    fs::create_dir_all(&plugin).unwrap();
    let reading = h.root.join("reading.json");
    fs::write(
        &reading,
        json!({ "plan": "max", "five": { "used": 42.4, "at": 1_900_000_000 }, "week": { "used": 7 } }).to_string(),
    )
    .unwrap();
    fs::write(
        plugin.join("plugin.json"),
        json!({ "api": 1, "id": "q", "name": "Q", "provides": ["quota"] }).to_string(),
    )
    .unwrap();
    fs::write(
        plugin.join("quota.json"),
        json!({ "api": 1, "sources": [{
            "agent": "claude", "file": reading.to_string_lossy(), "plan": "/plan",
            "windows": [
                { "minutes": 300, "used": "/five/used", "resets_at": "/five/at" },
                { "minutes": 10080, "used": "/week/used" }
            ]
        }] })
        .to_string(),
    )
    .unwrap();
    let mut r = Remote::open(&h, Some("view"), &[("XDG_CONFIG_HOME", config.as_path())]);
    let q = r.ok(json!({ "op": "quota" }));
    let agents = q["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1, "{q}");
    let a = &agents[0];
    assert_eq!(
        (a["agent"].as_str(), a["plan"].as_str()),
        (Some("claude"), Some("max"))
    );
    assert!(fibo_core::content::now().abs_diff(a["at"].as_u64().unwrap()) < 120);
    assert_eq!(
        a["windows"],
        json!([
            { "minutes": 300, "used": 42, "resets_at": 1_900_000_000u64, "level": "low" },
            { "minutes": 10080, "used": 7, "resets_at": 0, "level": "low" }
        ])
    );
}
