//! `toolcalls` and `toolcall` through `mooring remote --stdio`: a stretch's calls, a call in
//! full from its raw log, long output in pieces well under a line's limit, secrets hidden.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

/// A reply's line as the wire carries it: well under `MAX_LINE`, whatever it holds.
const MOST_REPLY: usize = 256 << 10;

fn user(cwd: &Path, u: &str, text: &str) -> Value {
    json!({"type":"user","uuid":u,"cwd":cwd,"entrypoint":"cli","userType":"external","message":{"content":text}})
}
fn call(u: &str, id: &str, name: &str, input: Value) -> Value {
    json!({"type":"assistant","uuid":u,"message":{"content":[{"type":"tool_use","id":id,"name":name,"input":input}]}})
}
fn result(u: &str, id: &str, content: Value) -> Value {
    json!({"type":"user","uuid":u,"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":content}]}})
}
fn said(u: &str, text: &str) -> Value {
    json!({"type":"assistant","uuid":u,"message":{"content":[{"type":"text","text":text}]}})
}

/// The messages once the host has read `want` of them.
fn messages(r: &mut Remote, terminal: &str, want: usize) -> Vec<Value> {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let page = r.ok(json!({ "op": "chat", "terminal": terminal, "tools": true, "limit": 500 }));
        let m = page["messages"].as_array().unwrap();
        if m.len() >= want {
            return m.clone();
        }
        assert!(Instant::now() < until, "chat never filled: {page}");
        std::thread::sleep(Duration::from_millis(100));
    }
}
/// More of a conversation, as a running CLI writes it: only a log's last 128 KiB is read when
/// the host first meets it, and from there on every line.
fn append(log: &Path, rows: &[Value]) {
    let id = log.file_stem().unwrap().to_string_lossy().into_owned();
    let mut text = String::new();
    for row in rows {
        let mut row = row.clone();
        row["sessionId"] = id.clone().into();
        row["timestamp"] = mooring_core::content::now().into();
        text += &format!("{row}\n");
    }
    let mut file = fs::OpenOptions::new().append(true).open(log).unwrap();
    file.write_all(text.as_bytes()).unwrap();
}
fn size(v: &Value) -> usize {
    v.to_string().len()
}

#[test]
fn a_stretch_lists_its_calls_and_a_call_opens_whole() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let secret = "ghp_0123456789abcdefghijklmnopqrstuv";
    let image = "QUJD".repeat(4000);
    let rows = [
        user(&work, "u1", "go"),
        call(
            "a1",
            "t1",
            "Bash",
            json!({"command":format!("curl -H 'Authorization: Bearer abc123def456' {secret}")}),
        ),
        result(
            "r1",
            "t1",
            json!(format!("token={secret}\nfine\u{1b}[31m red\u{1b}[0m")),
        ),
        call(
            "a2",
            "t2",
            "Edit",
            json!({"file_path":"/r/a.rs","old_string":"x","new_string":"y\nz"}),
        ),
        json!({"type":"user","uuid":"r2","message":{"content":[{"type":"tool_result","tool_use_id":"t2","is_error":true,"content":"no match"}]}}),
        call("a3", "t3", "Read", json!({"file_path":"/r/shot.png"})),
        result(
            "r3",
            "t3",
            json!([{"type":"image","source":{"data":image}}]),
        ),
        call(
            "a4",
            "t4",
            "Grep",
            json!({"pattern":"fn main","path":"src"}),
        ),
        said("a5", "done"),
        call("a6", "t6", "Bash", json!({"command":"later"})),
        said("a7", "more"),
    ];
    h.claude("c", &work, &rows, "sleep 60");
    let mut r = Remote::open(&h, Some("view"), &[]);
    assert!(r.ops().iter().any(|o| o == "toolcalls") && r.ops().iter().any(|o| o == "toolcall"));
    let m = messages(&mut r, "c", 3);
    assert_eq!(m[1]["tools"]["calls"], 4, "{}", m[1]);

    let list = r.ok(json!({ "op": "toolcalls", "terminal": "c", "seq": m[1]["seq"] }));
    let calls = list["calls"].as_array().unwrap();
    let ids: Vec<&str> = calls.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["t1", "t2", "t3", "t4"], "only this stretch: {list}");
    assert_eq!(list["tools"]["commands"], 1);
    assert!(list.get("next").is_none() && list.get("gone").is_none());
    assert_eq!(
        calls[0]["label"], "curl -H Authorization: Bearer",
        "a label is cleaned too"
    );
    assert_eq!(
        (calls[0]["kind"].as_str(), calls[0]["failed"].as_bool()),
        (Some("command"), Some(false))
    );
    assert_eq!(
        (calls[1]["label"].as_str(), calls[1]["failed"].as_bool()),
        (Some("/r/a.rs"), Some(true))
    );
    assert_eq!(
        calls[2]["size"],
        image.len() as u64,
        "the image counts toward its size"
    );
    assert_eq!(calls[3]["label"], "fn main · src");
    assert_eq!(calls[3]["open"], true, "no result logged");
    assert!(!list.to_string().contains(secret), "{list}");

    // The next stretch starts after the message between them.
    let later = r.ok(json!({ "op": "toolcalls", "terminal": "c", "seq": m[2]["seq"] }));
    assert_eq!(later["calls"][0]["id"], "t6", "{later}");

    let one = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "t1" }));
    let text = one.to_string();
    assert!(
        !text.contains(secret) && !text.contains("abc123def456"),
        "{text}"
    );
    assert_eq!(one["input_kind"], "command");
    assert_eq!(one["output"]["head"], "token=[已隐藏]\nfine red", "{one}");

    let edit = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "t2" }));
    assert_eq!(
        (edit["input_kind"].as_str(), edit["input"]["head"].as_str()),
        (Some("diff"), Some("- x\n+ y\n+ z\n"))
    );
    assert_eq!(edit["failed"], true);
    assert!(
        edit.get("code").is_none(),
        "an error flag is no exit code: {edit}"
    );

    let shot = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "t3" }));
    assert_eq!(
        shot["output"]["head"], "[1 张图片 · 16 KB，手机不显示]",
        "{shot}"
    );
    assert!(!shot.to_string().contains("QUJDQUJD"), "no base64");

    let open = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "t4" }));
    assert!(open.get("output").is_none(), "{open}");

    assert_eq!(
        r.error(json!({ "op": "toolcall", "terminal": "c", "call": "none" })),
        "这条工具调用不在记录里"
    );
    assert!(r
        .error(json!({ "op": "toolcalls" }))
        .contains("需要 terminal 或 session"));
}

/// The sizes real outputs reach, in the shapes that swell most when escaped: every reply
/// stays well under a line, and the pieces add up to the whole text.
#[test]
fn long_output_comes_in_pieces_far_below_a_lines_limit() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let plain = "line of ordinary output\n".repeat(30_000);
    let quotes = "\"\\".repeat(340_000);
    let controls = "\u{1}\u{2}\u{3}x y\n".repeat(170_000);
    let wide = "中文输出🙂\n".repeat(40_000);
    let long_input = "echo hi; ".repeat(60_000);
    let log = h.claude("c", &work, &[user(&work, "u1", "go")], "sleep 60");
    let mut r = Remote::open(&h, None, &[]);
    messages(&mut r, "c", 1);
    let mut rows = Vec::new();
    for (i, text) in [&plain, &quotes, &controls, &wide].iter().enumerate() {
        rows.push(call(
            &format!("a{i}"),
            &format!("t{i}"),
            "Bash",
            json!({"command":"cat big"}),
        ));
        rows.push(result(&format!("r{i}"), &format!("t{i}"), json!(text)));
    }
    rows.push(call("a9", "t9", "Bash", json!({"command":long_input})));
    rows.push(result("r9", "t9", json!("ok")));
    rows.push(said("z", "done"));
    append(&log, &rows);
    let m = messages(&mut r, "c", 2);
    let list = r.ok(json!({ "op": "toolcalls", "terminal": "c", "seq": m[1]["seq"] }));
    assert!(size(&list) < MOST_REPLY, "{}", size(&list));
    assert_eq!(list["calls"][0]["size"], plain.len() as u64);

    for (id, whole) in [
        ("t0", plain.clone()),
        ("t1", quotes.clone()),
        ("t2", "x y\n".repeat(170_000)),
        ("t3", wide.clone()),
    ] {
        let one = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": id }));
        assert!(size(&one) < MOST_REPLY, "{id}: {}", size(&one));
        let out = &one["output"];
        assert_eq!(out["total"], whole.len() as u64, "{id}");
        let mut text = out["head"].as_str().unwrap().to_owned();
        let tail_at = out["tail_at"].as_u64().unwrap() as usize;
        let mut fetched = 0;
        while text.len() < tail_at {
            let piece = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": id, "part": "output", "offset": text.len() }));
            assert!(size(&piece) < MOST_REPLY, "{id}: {}", size(&piece));
            assert_eq!(piece["offset"], text.len() as u64);
            text += piece["text"].as_str().unwrap();
            fetched += 1;
        }
        assert!(fetched > 0, "{id} should need more than its ends");
        assert_eq!(&text[..tail_at], &whole[..tail_at], "{id}");
        assert_eq!(&whole[tail_at..], out["tail"].as_str().unwrap(), "{id}");
    }
    // A line past what is ever held in memory is named, not read and not called gone.
    append(
        &log,
        &[
            user(&work, "u2", "again"),
            call("h", "t10", "Bash", json!({"command":"cat huge"})),
            result("hr", "t10", json!("y".repeat(33 << 20))),
            said("zz", "done"),
        ],
    );
    messages(&mut r, "c", 4);
    let huge = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "t10" }));
    assert_eq!(
        huge["output"]["head"], "[这条输出超过 32 MB，手机不显示]",
        "{huge}"
    );
    let long = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "t9" }));
    assert!(size(&long) < MOST_REPLY, "{}", size(&long));
    assert_eq!(
        long["input"]["total"],
        long_input.len() as u64,
        "the raw line has all of it"
    );
    let rest = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "t9", "part": "input", "offset": long["input"]["tail_at"] }));
    assert!(rest["text"].as_str().unwrap().ends_with("echo hi; "));
    assert!(r
        .error(json!({ "op": "toolcall", "terminal": "c", "call": "t9", "part": "nope" }))
        .contains("part"));
}

/// 2,007 calls in one stretch, as one real conversation has: pages, each short, in order.
#[test]
fn a_long_stretch_pages_through_every_call() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let log = h.claude("c", &work, &[user(&work, "u1", "go")], "sleep 60");
    let mut r = Remote::open(&h, None, &[]);
    messages(&mut r, "c", 1);
    let mut rows = Vec::new();
    for i in 0..2007 {
        rows.push(call(
            &format!("a{i}"),
            &format!("t{i}"),
            "Read",
            json!({"file_path":format!("/r/{i}.rs")}),
        ));
        rows.push(result(&format!("r{i}"), &format!("t{i}"), json!("ok")));
    }
    rows.push(said("z", "done"));
    append(&log, &rows);
    let m = messages(&mut r, "c", 2);
    let mut ids = Vec::new();
    let mut after = Value::Null;
    let mut pages = 0;
    loop {
        let mut ask =
            json!({ "op": "toolcalls", "terminal": "c", "seq": m[1]["seq"], "limit": 500 });
        if !after.is_null() {
            ask["after"] = after.clone();
        }
        let page = r.ok(ask);
        assert!(size(&page) < MOST_REPLY, "{}", size(&page));
        assert_eq!(
            page.get("tools").is_some(),
            pages == 0,
            "only the first page sums up"
        );
        ids.extend(
            page["calls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["id"].as_str().unwrap().to_owned()),
        );
        pages += 1;
        match page.get("next") {
            Some(next) => after = next.clone(),
            None => break,
        }
    }
    assert_eq!(pages, 11, "at most 200 to a page");
    let want: Vec<String> = (0..2007).map(|i| format!("t{i}")).collect();
    assert_eq!(ids, want);
}

/// With the log gone or replaced, it says so instead of showing nothing.
#[test]
fn a_raw_log_that_is_gone_says_so() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let rows = [
        user(&work, "u1", "go"),
        call("a1", "t1", "Bash", json!({"command":"cargo test"})),
        result("r1", "t1", json!("ok")),
        said("z", "done"),
    ];
    let log = h.claude("c", &work, &rows, "sleep 60");
    let mut r = Remote::open(&h, Some("answer"), &[]);
    let m = messages(&mut r, "c", 2);
    let session = r.ok(json!({ "op": "chat", "terminal": "c" }))["session"].clone();
    assert!(
        r.ok(json!({ "op": "toolcall", "session": session, "call": "t1" }))["output"].is_object()
    );
    let keep = log.with_extension("kept");
    fs::rename(&log, &keep).unwrap();
    let list = r.ok(json!({ "op": "toolcalls", "session": session, "seq": m[1]["seq"] }));
    assert_eq!(list["gone"], true, "{list}");
    assert_eq!(
        list["calls"][0]["label"], "cargo test",
        "what the index kept still names it"
    );
    assert!(list["calls"][0].get("size").is_none());
    assert_eq!(
        r.error(json!({ "op": "toolcall", "session": session, "call": "t1" })),
        "原始记录已不在"
    );
    // Another file now under that name: its offsets mean nothing, whichever the host notices
    // first (`Log::line` has the unit test).
    fs::write(&log, "not the same file\n").unwrap();
    let e = r.error(json!({ "op": "toolcall", "session": session, "call": "t1" }));
    assert!(
        e == "原始记录已不在" || e == "这条工具调用不在记录里",
        "{e}"
    );
}

/// A Codex rollout as real ones are: scripts, their commands logged apart, and function calls.
/// A long log the host first meets as it is: only its end is read. The chat says what is
/// missing, with messages in that end and with none, instead of looking like the start.
#[test]
fn a_log_read_only_from_its_end_says_so() {
    let id = |n: u8| format!("00000000-0000-4000-8000-00000000000{n}");
    let line = |id: &str, row: Value| {
        let mut row = row;
        row["sessionId"] = id.into();
        row["cwd"] = "/project".into();
        row["entrypoint"] = "cli".into();
        row["userType"] = "external".into();
        row["timestamp"] = mooring_core::content::now().into();
        format!("{row}\n")
    };
    let h = Host::start_over(|root| {
        let dir = root.join("logs/claude/project");
        fs::create_dir_all(&dir).unwrap();
        // Long, ending in words: what the end holds shows.
        let mut with = line(&id(1), user(Path::new("/project"), "u0", "很早的话"));
        with += &line(&id(1), said("a0", &"很早的回复".repeat(40_000)));
        with += &line(&id(1), user(Path::new("/project"), "u1", "最近的话"));
        with += &line(&id(1), said("a1", "最近的回复"));
        fs::write(dir.join(format!("{}.jsonl", id(1))), with).unwrap();
        // Long, its end all machine records: nothing there to show.
        let mut none = line(&id(2), user(Path::new("/project"), "u0", "很早的话"));
        let blob = json!({"type":"api-request-blob","blob":"x".repeat(200_000)});
        none += &line(&id(2), blob);
        fs::write(dir.join(format!("{}.jsonl", id(2))), none).unwrap();
    });
    let session = |h: &Host, cli: &str| {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let w = h.workspace();
            let found = w.agent_sessions.iter().find(|s| s.cli_session_id == cli);
            if let Some(s) = found {
                return s.id.clone();
            }
            assert!(Instant::now() < until, "the walk never found {cli}");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let (with, none) = (session(&h, &id(1)), session(&h, &id(2)));
    let mut r = Remote::open(&h, None, &[]);
    let page = r.ok(json!({ "op": "chat", "session": with }));
    let words: Vec<_> = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_owned())
        .collect();
    assert!(words.iter().any(|t| t == "最近的回复"), "{words:?}");
    assert!(
        !words.iter().any(|t| t.starts_with("很早的回复")),
        "{words:?}"
    );
    assert_eq!(page["partial"], true);
    let page = r.ok(json!({ "op": "chat", "session": none }));
    // Only the first ask, which a recent log's opening always gives, and no answer to it.
    assert_eq!(page["messages"].as_array().unwrap().len(), 1);
    assert_eq!(page["partial"], true);
}

/// Codex items that had no row: a phone sees each in the list and opens it, and an image is
/// only named. Shapes from real rollouts.
#[test]
fn codex_finished_items_are_rows_a_phone_can_open() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let done = |item: Value| json!({"type":"event_msg","timestamp":"2026-11-05T00:00:03Z","payload":{"type":"item_completed","item":item}});
    let first = json!({"type":"response_item","timestamp":"2026-11-05T00:00:01Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]}});
    let reply = json!({"type":"response_item","timestamp":"2026-11-05T00:00:09Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"好了"}]}});
    let rows = [
        first,
        done(
            json!({"type":"McpToolCall","id":"ex1","server":"crawlume","tool":"fetch","arguments":{"url":"https://e.com/a"},"status":"completed","duration":{"secs":2,"nanos":0},"result":{"content":[{"type":"text","text":"页面正文"},{"type":"image","data":"QUJDRA=="}],"isError":false}}),
        ),
        done(
            json!({"type":"FileChange","id":"ex2","status":"completed","stdout":"","stderr":"","changes":{"/r/old.rs":{"type":"update","unified_diff":"--- a/old.rs\n+++ b/old.rs\n@@ -1 +1 @@\n-x\n+y\n"}}}),
        ),
        done(json!({"type":"ImageView","id":"ex3","path":"/r/a.png"})),
        reply,
    ];
    h.codex("c", &work, &rows, "sleep 60");
    let mut r = Remote::open(&h, None, &[]);
    let m = messages(&mut r, "c", 2);
    assert_eq!(m[1]["tools"]["files"][0]["path"], "/r/old.rs");
    assert_eq!(m[1]["tools"]["files"][0]["added"], 1);
    assert_eq!(m[1]["tools"]["calls"], 3);
    let list = r.ok(json!({ "op": "toolcalls", "terminal": "c", "seq": m[1]["seq"] }));
    let calls = list["calls"].as_array().unwrap();
    let kinds: Vec<_> = calls
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap(),
                c["kind"].as_str().unwrap(),
                c["label"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("mcp__crawlume__fetch", "other", "https://e.com/a"),
            ("apply_patch", "edit", "/r/old.rs"),
            ("view_image", "read", "/r/a.png"),
        ]
    );
    assert_eq!(calls[0]["took_ms"], 2000);
    let one = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "ex1" }));
    let out = one["output"]["head"].as_str().unwrap();
    assert!(out.contains("页面正文") && out.contains("张图片"), "{out}");
    assert!(!out.contains("QUJDRA"), "{out}");
    let one = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "ex2" }));
    assert_eq!(one["input_kind"], "diff");
    assert!(one["input"]["head"].as_str().unwrap().contains("+y"));
    let one = r.ok(json!({ "op": "toolcall", "terminal": "c", "call": "ex3" }));
    assert!(one["output"]["head"].as_str().unwrap().contains("张图片"));
}

#[test]
fn codex_scripts_read_as_what_they_did() {
    let h = Host::start();
    let work = scratch(&h, "work");
    let item =
        |p: Value| json!({"type":"response_item","timestamp":"2026-11-05T00:00:01Z","payload":p});
    let done = |id: &str, cmd: &str, code: i64, out: &str| {
        json!({"type":"event_msg","timestamp":"2026-11-05T00:00:02Z","payload":{"type":"item_completed","item":{
        "type":"CommandExecution","id":id,"command":["/bin/zsh","-lc",cmd],"cwd":"/r","aggregated_output":out,"exit_code":code,"duration":{"secs":2,"nanos":500_000_000}}}})
    };
    let patch = "*** Begin Patch\\n*** Add File: /r/new.md\\n+hi\\n*** End Patch";
    let rows = [
        item(json!({"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]})),
        // Only commands, each logged apart: the script says nothing they do not.
        item(
            json!({"type":"custom_tool_call","call_id":"s1","name":"exec","input":"text(await tools.exec_command({cmd:\"cargo test\"}));"}),
        ),
        done("ex1", "cargo test", 101, "1 failed"),
        item(
            json!({"type":"custom_tool_call_output","call_id":"s1","output":[{"type":"input_text","text":"1 failed"}]}),
        ),
        // A patch has no row of its own; a command built at run time cannot be read.
        item(
            json!({"type":"custom_tool_call","call_id":"s2","name":"exec","input":format!("await tools.apply_patch(\"{patch}\");await tools.exec_command(opts);")}),
        ),
        done("ex2", "pwd", 0, "/r"),
        item(
            json!({"type":"custom_tool_call_output","call_id":"s2","output":[{"type":"input_text","text":"patched"},{"type":"input_image","image_url":"data:image/png;base64,QUJD"}]}),
        ),
        // An older log's function call, its output wrapped.
        item(
            json!({"type":"function_call","call_id":"f1","name":"exec_command","arguments":"{\"cmd\":\"ls\"}"}),
        ),
        item(
            json!({"type":"function_call_output","call_id":"f1","output":"{\"output\":\"a.rs\",\"metadata\":{\"exit_code\":0}}"}),
        ),
        item(
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}),
        ),
    ];
    h.codex("x", &work, &rows, "sleep 60");
    let mut r = Remote::open(&h, None, &[]);
    let m = messages(&mut r, "x", 2);
    let list = r.ok(json!({ "op": "toolcalls", "terminal": "x", "seq": m[1]["seq"] }));
    let calls = list["calls"].as_array().unwrap();
    let got: Vec<(&str, &str)> = calls
        .iter()
        .map(|c| (c["id"].as_str().unwrap(), c["label"].as_str().unwrap()))
        .collect();
    assert_eq!(
        got,
        [
            ("ex1", "cargo test"),
            ("s2", "改 new.md；运行时才拼出的命令，读不到内容"),
            ("ex2", "pwd"),
            ("f1", "ls"),
        ],
        "{list}"
    );
    assert_eq!(
        (calls[0]["failed"].as_bool(), calls[0]["code"].as_i64()),
        (Some(true), Some(101))
    );
    assert_eq!(calls[0]["took_ms"], 2500);
    assert_eq!(calls[3]["failed"], false);
    let run = r.ok(json!({ "op": "toolcall", "terminal": "x", "call": "ex1" }));
    assert_eq!(
        (
            run["input"]["head"].as_str(),
            run["output"]["head"].as_str()
        ),
        (Some("cargo test"), Some("1 failed"))
    );
    let script = r.ok(json!({ "op": "toolcall", "terminal": "x", "call": "s2" }));
    assert_eq!(script["input_kind"], "code");
    assert!(
        script["note"].as_str().unwrap().contains("1 步"),
        "{script}"
    );
    assert_eq!(script["steps"][1]["unread"], true);
    assert_eq!(
        script["output"]["head"], "patched\n[1 张图片 · 1 KB，手机不显示]",
        "{script}"
    );
    let old = r.ok(json!({ "op": "toolcall", "terminal": "x", "call": "f1" }));
    assert_eq!(old["output"]["head"], "a.rs");
}

/// What a real log costs, timings only: `MOORING_TOOLS_LOG=<path> [MOORING_TOOLS_CODEX=1]
/// cargo test --release -p mooring-host --test remote_tools -- --ignored --nocapture
/// real_log_costs`. The log is copied into the test's own runtime, which is deleted at the
/// end; nothing from it is printed. It grows into place as a running CLI's does, so every
/// row exists.
#[test]
#[ignore = "reads a real log; prints timings only"]
fn real_log_costs() {
    use std::io::Read;
    let source = std::env::var("MOORING_TOOLS_LOG").expect("MOORING_TOOLS_LOG");
    let codex = std::env::var_os("MOORING_TOOLS_CODEX").is_some();
    let h = Host::start();
    let work = scratch(&h, "work");
    let first =
        json!({"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]});
    let log = if codex {
        let item =
            json!({"type":"response_item","timestamp":"2026-11-05T00:00:01Z","payload":first});
        h.codex("r", &work, &[item], "sleep 3000")
    } else {
        h.claude("r", &work, &[user(&work, "u1", "go")], "sleep 3000")
    };
    let len = fs::metadata(&source).unwrap().len();
    let began = Instant::now();
    {
        let mut file = fs::File::open(&source).unwrap();
        let mut out = fs::OpenOptions::new().append(true).open(&log).unwrap();
        let mut buf = vec![0; 8 << 20];
        loop {
            let n = file.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).unwrap();
        }
    }
    let mut r = Remote::open(&h, None, &[]);
    // Read through: the message count stops moving.
    let (mut seen, mut still) = (0, 0);
    let patience = if len > 1 << 30 { 500 } else { 8 };
    while still < patience {
        let page = r.ok(json!({ "op": "chat", "terminal": "r", "limit": 500 }));
        let n = page["messages"]
            .as_array()
            .unwrap()
            .last()
            .map_or(0, |m| m["seq"].as_i64().unwrap());
        still = if n == seen { still + 1 } else { 0 };
        seen = n;
        std::thread::sleep(Duration::from_millis(500));
    }
    println!(
        "{} MB read in {:.1} s",
        len >> 20,
        began.elapsed().as_secs_f64()
    );

    // The host's own latency beside it, as a terminal's reader would feel its lock.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let probe = {
        let (socket, stop) = (h.socket(), stop.clone());
        std::thread::spawn(move || {
            let mut took = Vec::new();
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let t = Instant::now();
                let _ = mooring_core::protocol::rpc(
                    &socket,
                    &mooring_core::protocol::Request::ReadScreen {
                        id: "r".into(),
                        rows: 5,
                    },
                );
                took.push(t.elapsed().as_secs_f64() * 1000.0);
                std::thread::sleep(Duration::from_millis(5));
            }
            took
        })
    };
    let quantile = |v: &mut Vec<f64>, q: f64| {
        v.sort_by(f64::total_cmp);
        v.get(((v.len() as f64 * q) as usize).min(v.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0)
    };
    std::thread::sleep(Duration::from_secs(3));
    let mut before = {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        probe.join().unwrap()
    };
    println!(
        "host reply alone: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
        quantile(&mut before, 0.5),
        quantile(&mut before, 0.99),
        quantile(&mut before, 1.0)
    );
    stop.store(false, std::sync::atomic::Ordering::Relaxed);
    let probe = {
        let (socket, stop) = (h.socket(), stop.clone());
        std::thread::spawn(move || {
            let mut took = Vec::new();
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let t = Instant::now();
                let _ = mooring_core::protocol::rpc(
                    &socket,
                    &mooring_core::protocol::Request::ReadScreen {
                        id: "r".into(),
                        rows: 5,
                    },
                );
                took.push(t.elapsed().as_secs_f64() * 1000.0);
                std::thread::sleep(Duration::from_millis(5));
            }
            took
        })
    };

    // Every stretch the newest messages reach, most calls first.
    let page = r.ok(json!({ "op": "chat", "terminal": "r", "limit": 500, "tools": true }));
    let mut stretches: Vec<(u64, i64)> = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| Some((m["tools"]["calls"].as_u64()?, m["seq"].as_i64()?)))
        .collect();
    stretches.sort_by_key(|s| std::cmp::Reverse(s.0));
    println!(
        "messages {} · stretches with calls {} · most calls {}",
        page["messages"].as_array().unwrap().len(),
        stretches.iter().filter(|s| s.0 > 0).count(),
        stretches.first().map_or(0, |s| s.0)
    );
    let (mut list_ms, mut one_ms) = (Vec::new(), Vec::new());
    let mut largest_reply = 0usize;
    let mut by_size: Vec<(u64, String)> = Vec::new();
    let busy_until = Instant::now() + Duration::from_secs(6);
    let mut rounds = 0;
    while rounds == 0 || Instant::now() < busy_until {
        rounds += 1;
        for (_, seq) in stretches.iter().take(25) {
            let mut after = Value::Null;
            loop {
                let mut ask = json!({ "op": "toolcalls", "terminal": "r", "seq": seq });
                if !after.is_null() {
                    ask["after"] = after.clone();
                }
                let t = Instant::now();
                let list = r.ok(ask);
                list_ms.push(t.elapsed().as_secs_f64() * 1000.0);
                largest_reply = largest_reply.max(size(&list));
                if rounds == 1 {
                    for c in list["calls"].as_array().unwrap() {
                        by_size.push((
                            c["size"].as_u64().unwrap_or(0),
                            c["id"].as_str().unwrap().to_owned(),
                        ));
                    }
                }
                match list.get("next") {
                    Some(n) => after = n.clone(),
                    None => break,
                }
            }
        }
    }
    by_size.sort_by_key(|c| std::cmp::Reverse(c.0));
    // The longest text among the largest outputs: images among them are only a note.
    let mut longest = (0u64, String::new());
    for (_, id) in by_size.iter().take(40) {
        let t = Instant::now();
        let one = r.ok(json!({ "op": "toolcall", "terminal": "r", "call": id }));
        one_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        largest_reply = largest_reply.max(size(&one));
        let total = one["output"]["total"].as_u64().unwrap_or(0);
        if total > longest.0 {
            longest = (total, id.clone());
        }
    }
    let mut slice_ms = Vec::new();
    if !longest.1.is_empty() {
        let one = r.ok(json!({ "op": "toolcall", "terminal": "r", "call": longest.1 }));
        let mut at = one["output"]["head"].as_str().map_or(0, str::len) as u64;
        let end = one["output"]["tail_at"].as_u64().unwrap_or(0);
        while at < end {
            let t = Instant::now();
            let piece = r.ok(json!({ "op": "toolcall", "terminal": "r", "call": longest.1, "part": "output", "offset": at }));
            slice_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            largest_reply = largest_reply.max(size(&piece));
            at = piece["next"].as_u64().unwrap();
        }
    }
    let mut during = {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        probe.join().unwrap()
    };
    println!(
        "list pages {}: p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        list_ms.len(),
        quantile(&mut list_ms, 0.5),
        quantile(&mut list_ms, 0.95),
        quantile(&mut list_ms, 1.0)
    );
    println!(
        "largest output as logged {} KB · single calls {}: p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        by_size.first().map_or(0, |c| c.0) >> 10,
        one_ms.len(),
        quantile(&mut one_ms, 0.5),
        quantile(&mut one_ms, 0.95),
        quantile(&mut one_ms, 1.0)
    );
    println!(
        "longest text {} KB in {} stretches: p50 {:.1} ms, max {:.1} ms",
        longest.0 >> 10,
        slice_ms.len(),
        quantile(&mut slice_ms, 0.5),
        quantile(&mut slice_ms, 1.0)
    );
    println!("largest reply {} KB", largest_reply >> 10);
    println!(
        "host reply meanwhile ({} asks): p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
        during.len(),
        quantile(&mut during, 0.5),
        quantile(&mut during, 0.99),
        quantile(&mut during, 1.0)
    );
}
