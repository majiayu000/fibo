//! A demo machine with everything the phone's new fields show, for looking at the app:
//! `cargo test -p mooring-host --test phone_gaps_demo -- --ignored --exact phone_gaps_demo --nocapture`
//! then paste the printed link into `cd mobile && bunx expo start --web`. Fake Agents, real
//! host, real relay on 127.0.0.1:8790, and a `HOME` of its own with real git repositories.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use mooring_core::{model::*, protocol::*};
use serde_json::{json, Value};
use std::fs;
use std::process::{Command, Stdio};

fn user(cwd: &str, text: &str) -> Value {
    json!({"type":"user","uuid":uuid::Uuid::new_v4().to_string(),"cwd":cwd,"entrypoint":"cli","userType":"external","permissionMode":"default","message":{"content":text}})
}
fn timed(mut v: Value, at: u64) -> Value {
    v["timestamp"] = chrono::DateTime::from_timestamp(at as i64, 0)
        .unwrap()
        .to_rfc3339()
        .into();
    v
}
fn call(id: &str, name: &str, input: Value) -> Value {
    json!({"type":"assistant","uuid":format!("a-{id}"),"message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","id":id,"name":name,"input":input}]}})
}
fn result(id: &str, text: &str) -> Value {
    json!({"type":"user","uuid":format!("r-{id}"),"toolUseResult":{},"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":text}]}})
}
fn said(text: &str, last: bool) -> Value {
    let stop = if last { "end_turn" } else { "tool_use" };
    json!({"type":"assistant","uuid":uuid::Uuid::new_v4().to_string(),"message":{"model":"claude-opus-5-5","stop_reason":stop,"content":[{"type":"text","text":text}]}})
}

#[test]
#[ignore = "manual: a demo machine for the phone app"]
fn phone_gaps_demo() {
    // A long conversation the walk finds on its own, so only its end is read at first: what
    // the phone reads back when it scrolls to the top.
    let long_id = uuid::Uuid::new_v4().to_string();
    let long_log = |root: &std::path::Path| {
        let dir = root.join("logs/claude");
        fs::create_dir_all(&dir).unwrap();
        let mut log = String::new();
        for n in 0..600 {
            let pad = "。".repeat(1500);
            let row = |v: Value| {
                let mut v = v;
                v["sessionId"] = long_id.clone().into();
                v["timestamp"] = mooring_core::content::now().into();
                format!("{v}\n")
            };
            log += &row(user("/project", &format!("第 {n} 句话{pad}")));
            log += &row(said(&format!("这是第 {n} 句的回复"), true));
        }
        fs::write(dir.join(format!("{long_id}.jsonl")), log).unwrap();
    };
    let h = Host::start_over(long_log);
    let home = scratch(&h, "home");
    let repo = home.join("code/mooring");
    fs::create_dir_all(repo.join("src/app")).unwrap();
    fs::create_dir_all(home.join("code/blog")).unwrap();
    fs::create_dir_all(home.join("code/scratch")).unwrap();
    git(&repo, &["init", "-q"]);
    let chat_tsx = "export const a = 1;\nexport const b = 2;\n";
    fs::write(repo.join("src/app/chat.tsx"), chat_tsx).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "one"]);
    let wt = |name: &str, branch: &str| {
        let dir = home.join(format!("code/{name}"));
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                branch,
                &dir.to_string_lossy(),
            ],
        );
        dir.to_string_lossy().into_owned()
    };
    let (stripes, pty) = (
        wt("mooring-quota", "feat/quota-stripes"),
        wt("mooring-pty", "fix/pty-ci"),
    );
    git(&home.join("code/blog"), &["init", "-q"]);
    fs::write(
        repo.join("src/app/chat.tsx"),
        "export const a = 1;\n// Android draws edge to edge.\nexport const b = 3;\n",
    )
    .unwrap();
    let main = repo.to_string_lossy().into_owned();
    let blog = home.join("code/blog").to_string_lossy().into_owned();
    let now = mooring_core::content::now();
    let space = |id: &str, name: &str, cwd: &str| {
        let edit = Edit::AddSpace(Space {
            id: id.into(),
            name: name.into(),
            cwd: cwd.into(),
        });
        h.call(Request::Edit {
            request: None,
            edit,
        })
        .unwrap();
    };
    // What `Host::start` made holds every fake Agent below (`Host::launch`).
    let edit = Edit::UpdateSpace(Space {
        id: "s".into(),
        name: "mooring".into(),
        cwd: main.clone(),
    });
    h.call(Request::Edit {
        request: None,
        edit,
    })
    .unwrap();
    space("s-blog", "blog", &blog);

    // A conversation that ended, for 「接着上次的对话」.
    let notes = home.join("code/notes");
    fs::create_dir_all(&notes).unwrap();
    git(&notes, &["init", "-q"]);
    let notes_dir = notes.to_string_lossy().into_owned();
    h.claude(
        "ended",
        &notes,
        &[
            timed(user(&notes_dir, "把审批框换成等宽字体"), now - 7200),
            timed(said("换好了。", true), now - 7100),
        ],
        "sleep 60",
    );
    h.call(Request::Stop { id: "ended".into() }).unwrap();
    // The approval.
    h.claude(
        "approve",
        &repo,
        &[timed(user(&main, "把 main 推到远端"), now - 120), timed(said("本地检查都过了，准备推送。", false), now - 110)],
        "sleep 2; printf '%s\\n' 'Bash command' '  git push origin main' 'Do you want to proceed?' '> 1. Yes' '  2. No'; read -r answer; printf '\\033[2J\\033[Hpushed\\n'; sleep 3600",
    );
    // Asking through its question tool.
    h.claude(
        "ask",
        std::path::Path::new(&stripes),
        &[
            timed(user(&stripes, "侧栏额度条换成竖条纹"), now - 400),
            call("q1", "AskUserQuestion", json!({"questions":[{"question":"诊断页也一起换吗？","options":[{"label":"都换"},{"label":"只换侧栏"}]}]})),
        ],
        "sleep 3600",
    );
    // Stopped on an error.
    h.claude(
        "failed",
        std::path::Path::new(&pty),
        &[
            timed(user(&pty, "cargo test 失败 3 个用例"), now - 900),
            json!({"type":"assistant","isApiErrorMessage":true,"message":{"model":"<synthetic>","content":[{"type":"text","text":"API Error: 500"}]}}),
        ],
        "sleep 3600",
    );
    // In the middle of a long turn: its tools, a test run after the edit, an idle waiter for send.
    let echo = "while read -r line; do :; done";
    h.claude(
        "running",
        &repo,
        &[
            json!({"type":"permission-mode","permissionMode":"default"}),
            timed(user(&main, "手机端键盘挡住输入框"), now - 720),
            timed(
                call("b1", "Bash", json!({"command":"./gradlew assembleRelease"})),
                now - 700,
            ),
            timed(result("b1", "BUILD SUCCESSFUL"), now - 500),
        ],
        echo,
    );
    h.claude(
        "writing",
        std::path::Path::new(&blog),
        &[
            json!({"type":"permission-mode","permissionMode":"bypassPermissions"}),
            timed(user(&blog, "整理十月的周报草稿"), now - 1560),
        ],
        echo,
    );
    // Finished: the tool line, the diff's test result.
    let review = h.claude(
        "review",
        &repo,
        &[
            json!({"type":"permission-mode","permissionMode":"acceptEdits"}),
            timed(user(&main, "安卓上键盘弹出来把输入框挡住了，看不到自己在打什么"), now - 300),
            timed(call("c1", "Bash", json!({"command":"bun test"})), now - 290),
            timed(result("c1", "8 pass 0 fail"), now - 280),
            timed(call("e1", "Edit", json!({"file_path":"src/app/chat.tsx","old_string":"x","new_string":"y\nz\nw\nv\nu\nt"})), now - 270),
            timed(result("e1", "ok"), now - 260),
            timed(call("c2", "Bash", json!({"command":"bunx tsc --noEmit"})), now - 200),
            timed(result("c2", "ok"), now - 195),
            timed(call("c3", "Bash", json!({"command":"cargo test -p mooring-host"})), now - 190),
            timed(result("c3", "test result: ok. 8 passed; 0 failed"), now - 185),
            timed(said("RN 0.88 在 Android 上强制全屏绘制，键盘弹出时窗口不再缩小。改了两处。\n\n类型检查通过。要我重新打包装到手机上吗？", true), now - 180),
        ],
        echo,
    );
    // Then a stretch with what a tool's detail has to cope with, as a running CLI writes it:
    // a failure, a long output and a long line, an image, and secrets to hide (made up here).
    let id = review.file_stem().unwrap().to_string_lossy().into_owned();
    let test_log: String = (1..=30_000)
        .map(|i| format!("test tool_text::tests::case_{i:05} ... ok\n"))
        .collect();
    let wide = format!(
        "{}\n{}\n",
        "column ".repeat(400),
        "长行中文输出".repeat(120)
    );
    let extra = [
        timed(user(&main, "再跑一遍全量测试"), now - 170),
        timed(call("d1", "Bash", json!({"command":"cargo test --workspace --no-fail-fast 2>&1 | tail -n 200"})), now - 165),
        timed(json!({"type":"user","uuid":"r-d1","message":{"content":[{"type":"tool_result","tool_use_id":"d1","is_error":true,"content":"error: test failed, to rerun pass `-p mooring-host --test managed_session`\nExit code 101"}]}}), now - 140),
        timed(call("d2", "Bash", json!({"command":"cat target/test-output.log"})), now - 139),
        timed(result("d2", &test_log), now - 130),
        timed(call("d3", "Read", json!({"file_path":"src/app/chat.tsx"})), now - 129),
        timed(result("d3", "1\texport const a = 1;\n2\t// Android draws edge to edge.\n3\texport const b = 3;\n"), now - 129),
        timed(call("d4", "Edit", json!({"file_path":"src/app/chat.tsx","old_string":"export const b = 2;","new_string":"// Android draws edge to edge.\nexport const b = 3;"})), now - 128),
        timed(result("d4", "ok"), now - 128),
        timed(call("d5", "Bash", json!({"command":"curl -H 'Authorization: Bearer demo1234token5678' https://api.example.com/v1/items"})), now - 127),
        timed(result("d5", "GITHUB_TOKEN=ghp_demo0123456789abcdefghijklmnopqr\npostgres://me:hunter2@db.local/app\n{\"ok\":true}\n"), now - 126),
        timed(call("d6", "Read", json!({"file_path":"docs/shot.png"})), now - 125),
        timed(json!({"type":"user","uuid":"r-d6","message":{"content":[{"type":"tool_result","tool_use_id":"d6","content":[{"type":"image","source":{"data":"QUJD".repeat(60_000)}}]}]}}), now - 125),
        timed(call("d7", "Grep", json!({"pattern":"KeyboardAvoidingView","path":"src/app"})), now - 124),
        timed(result("d7", "src/app/chat.tsx:6:import { KeyboardAvoidingView } from 'react-native';\n"), now - 124),
        timed(call("d8", "Bash", json!({"command":"cat wide.txt"})), now - 123),
        timed(result("d8", &wide), now - 123),
        timed(call("d9", "WebFetch", json!({"url":"https://docs.expo.dev/versions/v58.0.0/"})), now - 122),
        timed(result("d9", "Expo SDK 58"), now - 122),
        timed(said("全量测试里有一个失败，日志在上面。", true), now - 120),
    ];
    let mut text = String::new();
    for mut row in extra {
        row["sessionId"] = id.clone().into();
        text += &format!("{row}\n");
    }
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().append(true).open(&review).unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    // In tabs, as the window keeps them: only those count as 要你处理.
    for t in ["approve", "ask", "failed", "running", "writing", "review"] {
        let edit = Edit::CreateTab {
            tab: Tab {
                id: format!("tab-{t}"),
                space: "s".into(),
                default_task: None,
                title: String::new(),
                layout: Some(mooring_core::layout::Layout::Leaf(t.into())),
            },
        };
        h.call(Request::Edit {
            request: None,
            edit,
        })
        .unwrap();
    }
    let todo = |id: &str, space: &str, title: &str, agent| {
        let edit = Edit::AddTask(Task {
            id: id.into(),
            space: space.into(),
            title: title.into(),
            description: String::new(),
            acceptance: vec![],
            agent,
        });
        h.call(Request::Edit {
            request: None,
            edit,
        })
        .unwrap();
    };
    todo("t1", "s", "把看板待办接到手机", Agent::Claude);
    todo("t2", "s", "审批详情补后果说明", Agent::Codex);
    todo("t3", "s-blog", "写十一月的周报", Agent::Claude);

    // Today's tokens, as each CLI logs them; what `open` runs, a fake on PATH.
    let usage = |dir: &str, name: &str, lines: Vec<String>| {
        let dir = h.root.join(dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(name), lines.join("\n") + "\n").unwrap();
    };
    let at = timed(json!({}), now - 60)["timestamp"].clone();
    usage(
        "claude-home/projects/p",
        "a.jsonl",
        (0..40)
            .map(|i| json!({"type":"assistant","timestamp":at,"message":{"id":format!("m{i}"),"usage":{"input_tokens":120,"cache_creation_input_tokens":12000,"cache_read_input_tokens":900000,"output_tokens":8000}}}).to_string())
            .collect(),
    );
    usage(
        "codex-home/sessions/2026/11/05",
        "r.jsonl",
        vec![json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":3400000},"last_token_usage":{"input_tokens":3300000,"cached_input_tokens":2400000,"output_tokens":100000}}}}).to_string()],
    );
    let bin = scratch(&h, "bin");
    let claude = bin.join("claude");
    fs::write(
        &claude,
        "#!/bin/sh\necho \"fake claude: $*\"\nexec sleep 3600\n",
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

    let (ready, bound) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:8790")
                .await
                .unwrap();
            ready.send(()).unwrap();
            let _ = mooring_relay::server::serve(listener, Default::default()).await;
        });
    });
    bound.recv().unwrap();
    // The real screens the phone's approval and trust cards are about: 11-05's real run.
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens");
    let draw = |cli: &str, name: &str| {
        let file = fixtures.join(cli).join(name);
        format!(
            "stty -echo; printf '\\033[2J\\033[H'; cat '{}'; sleep 100000",
            file.display()
        )
    };
    h.claude(
        "real-push",
        &repo,
        &[user("/project", "把 main 推到远端")],
        &draw("claude", "approval-push-requires.txt"),
    );
    h.launch("long-screen", &repo, "seq 1 60; sleep 100000");
    h.launch(
        "long-history",
        &repo,
        &format!(
            "exec {} -c 'sleep 100000; :' fixture --session-id {long_id}",
            h.root.join("claude").display()
        ),
    );
    // A running CLI binds to its log when the log moves.
    let long_path = h.root.join(format!("logs/claude/{long_id}.jsonl"));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while h.workspace().current_agent("long-history").is_none() {
        assert!(
            std::time::Instant::now() < until,
            "long-history never bound"
        );
        std::thread::sleep(std::time::Duration::from_millis(500));
        let mut row = said("还在", true);
        row["sessionId"] = long_id.clone().into();
        row["timestamp"] = mooring_core::content::now().into();
        use std::io::Write;
        writeln!(
            fs::OpenOptions::new()
                .append(true)
                .open(&long_path)
                .unwrap(),
            "{row}"
        )
        .unwrap();
    }
    h.launch("trust-claude", &repo, &draw("claude", "trust-real.txt"));
    h.launch("trust-codex", &repo, &draw("codex", "trust-real.txt"));
    let base = "ws://127.0.0.1:8790";
    let root = h.root.to_string_lossy().into_owned();
    let paired = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args([
            "remote",
            "--pair",
            "--relay",
            base,
            "--name",
            "演示手机",
            "--role",
            "full",
            "--runtime",
            &root,
        ])
        .output()
        .unwrap();
    let printed = String::from_utf8(paired.stdout).unwrap();
    let link = printed
        .lines()
        .find(|l| l.starts_with("mooring://pair?"))
        .expect("pairing link");
    println!("PAIRING LINK: {link}");
    println!("RUNTIME: {root}");
    println!("HOME: {}", home.display());
    // Killed, it comes back after a few seconds: for trying a phone's reconnect.
    loop {
        let mut machine = Command::new(env!("CARGO_BIN_EXE_mooring"))
            .args(["remote", "--relay", base, "--runtime", &root])
            .env("HOME", &home)
            .env("SHELL", &shell)
            .env("CLAUDE_CONFIG_DIR", h.root.join("claude-home"))
            .env("CODEX_HOME", h.root.join("codex-home"))
            .stderr(Stdio::inherit())
            .spawn_tied()
            .unwrap();
        machine.wait().ok();
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
