use super::*;

/// A phone's chat view: `chat` over `mooring remote --stdio` gives a conversation's user
/// and Agent messages, the newest first page, then only newer ones with `after` and older
/// ones with `before`, named by the terminal running it or by its session.
#[test]
fn remote_chat_pages_a_conversation_by_terminal_and_session() {
    use serde_json::{json, Value};
    use std::io::{BufRead, Write};
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let path = logs.join(format!("{id}.jsonl"));
    let row = |i: usize, user: bool, text: &str| {
        let at = mooring_core::content::now();
        if user {
            json!({"type":"user","sessionId":id,"uuid":format!("u{i}"),"cwd":"/project","entrypoint":"cli","userType":"external","timestamp":at,"message":{"content":text}})
        } else {
            json!({"type":"assistant","sessionId":id,"uuid":format!("a{i}"),"timestamp":at,"message":{"content":[{"type":"text","text":text}]}})
        }
    };
    let seed = h.root.join("seed");
    let mut lines = String::new();
    for i in 0..3 {
        lines += &format!("{}\n", row(i, true, &format!("问题 {i}")));
        lines += &format!("{}\n", row(i, false, &format!("回答 {i}")));
    }
    fs::write(&seed, lines).unwrap();
    let more = h.root.join("more");
    fs::write(&more, format!("{}\n", row(9, false, "后来的回答"))).unwrap();
    let script = format!(
        "cat {} > {}; read -r go; cat {} >> {}; read -r end",
        shell_quote(&seed.to_string_lossy()),
        shell_quote(&path.to_string_lossy()),
        shell_quote(&more.to_string_lossy()),
        shell_quote(&path.to_string_lossy()),
    );
    h.call(h.launch(
        "chat",
        &format!(
            "exec {} -c {} fixture --session-id {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while h.workspace().current_agent("chat").is_none() {
        assert!(Instant::now() < until, "explicit ID must bind");
        std::thread::sleep(Duration::from_millis(50));
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--stdio", "--runtime", &h.root.to_string_lossy()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = io::BufReader::new(child.stdout.take().unwrap()).lines();
    let mut n = 0;
    let mut ask = |v: Value| -> Value {
        n += 1;
        let mut v = v;
        v["id"] = n.into();
        writeln!(stdin, "{v}").unwrap();
        loop {
            let line: Value = serde_json::from_str(&out.next().unwrap().unwrap()).unwrap();
            if line["id"] == n {
                return line;
            }
        }
    };
    let texts = |v: &Value| -> Vec<String> {
        v["ok"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                format!(
                    "{}:{}",
                    m["role"].as_str().unwrap(),
                    m["text"].as_str().unwrap()
                )
            })
            .collect()
    };
    let hello = ask(json!({ "op": "hello" }));
    assert!(hello["ok"]["ops"]
        .as_array()
        .unwrap()
        .contains(&json!("chat")));
    // The index may still be catching up with the log just written.
    let until = Instant::now() + Duration::from_secs(30);
    let page = loop {
        let page = ask(json!({ "op": "chat", "terminal": "chat", "limit": 4 }));
        if texts(&page).len() == 4 {
            break page;
        }
        assert!(Instant::now() < until, "chat never filled: {page}");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(
        texts(&page),
        ["user:问题 1", "agent:回答 1", "user:问题 2", "agent:回答 2"]
    );
    assert_eq!(page["ok"]["more"], true);
    let session = page["ok"]["session"].as_str().unwrap().to_owned();
    let first = page["ok"]["messages"][0]["seq"].as_i64().unwrap();
    let last = page["ok"]["messages"][3]["seq"].as_i64().unwrap();
    let older = ask(json!({ "op": "chat", "session": session, "before": first }));
    assert_eq!(texts(&older), ["user:问题 0", "agent:回答 0"]);
    assert_eq!(older["ok"]["more"], false);
    assert!(texts(&ask(
        json!({ "op": "chat", "session": session, "after": last })
    ))
    .is_empty());

    // The Agent replies again: `after` gives only that.
    h.call(Request::SendText {
        id: "chat".into(),
        text: String::new(),
        submit: true,
    })
    .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let newer = ask(json!({ "op": "chat", "session": session, "after": last }));
        if !texts(&newer).is_empty() {
            assert_eq!(texts(&newer), ["agent:后来的回答"]);
            break;
        }
        assert!(Instant::now() < until, "the new reply never came");
        std::thread::sleep(Duration::from_millis(100));
    }
    let bad = ask(json!({ "op": "chat" }));
    assert!(bad["error"]
        .as_str()
        .unwrap()
        .contains("terminal 或 session"));
    drop(ask);
    child.kill().ok();
    child.wait().ok();
}

/// The phone app's own client (`next/mobile/src/remote`, TypeScript) against this machine
/// through a real relay: Noise handshake, hello, events, list, chat, a message sent into
/// the conversation and the Agent's reply read back. Needs bun and `bun install` in
/// `next/mobile`, so it runs only when asked: `cargo test … -- --ignored the_js_phone`.
#[test]
#[ignore = "needs bun and next/mobile's node_modules"]
fn the_js_phone_client_chats_through_a_real_relay() {
    use serde_json::{json, Value};
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let path = logs.join(format!("{id}.jsonl"));
    let at = mooring_core::content::now();
    let user = json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/project","entrypoint":"cli","userType":"external","timestamp":at,"message":{"content":"帮我看看测试"}});
    let agent = |uuid: &str, text: &str| json!({"type":"assistant","sessionId":id,"uuid":uuid,"timestamp":at,"message":{"content":[{"type":"text","text":text}]}});
    let seed = h.root.join("seed");
    fs::write(&seed, format!("{user}\n{}\n", agent("a0", "测试都过了"))).unwrap();
    let reply = h.root.join("reply");
    fs::write(&reply, format!("{}\n", agent("a1", "收到手机的消息"))).unwrap();
    let got = h.root.join("got");
    let script = format!(
        "cat {seed} > {log}; read -r line; printf '%s' \"$line\" > {got}; cat {reply} >> {log}; while read -r more; do printf '%s' \"$more\" >> {got}; done",
        seed = shell_quote(&seed.to_string_lossy()),
        log = shell_quote(&path.to_string_lossy()),
        got = shell_quote(&got.to_string_lossy()),
        reply = shell_quote(&reply.to_string_lossy()),
    );
    h.call(h.launch(
        "phone-chat",
        &format!(
            "exec {} -c {} fixture --session-id {}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote(&script),
            id
        ),
    ))
    .unwrap();

    let (base, _stop, _relay) = local_relay("127.0.0.1:0", Default::default());
    let root = h.root.to_string_lossy().into_owned();
    let link = pair(&root, &base, "JS 手机", "full");
    let _machine = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--relay", &base, "--runtime", &root])
        .stderr(Stdio::null())
        .spawn_tied()
        .unwrap();
    // The machine registers with the relay before a phone can reach it.
    let until = Instant::now() + Duration::from_secs(10);
    while Phone::try_connect(&link).is_err() {
        assert!(Instant::now() < until, "machine never reached the relay");
        std::thread::sleep(Duration::from_millis(50));
    }

    let mobile = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../mobile");
    let out = Command::new(std::env::var("MOORING_BUN").unwrap_or_else(|_| "bun".into()))
        .current_dir(&mobile)
        .args(["test/phone-e2e.ts", &link.text(), "phone-chat"])
        .output()
        .expect("bun");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let seen: Value = serde_json::from_slice(&out.stdout).unwrap();
    eprintln!("js phone: {seen}");
    assert_eq!(seen["role"], "full");
    assert!(seen["ops"].as_array().unwrap().contains(&json!("chat")));
    assert_eq!(seen["card"]["id"], "phone-chat");
    assert_eq!(
        seen["before"],
        json!(["user:帮我看看测试", "agent:测试都过了"])
    );
    assert_eq!(seen["after"], json!(["agent:收到手机的消息"]));
    assert!(seen["bad"]
        .as_str()
        .unwrap()
        .contains("terminal 或 session"));
    assert_eq!(seen["pong"], json!({}));
    // Sent again under the same request id after a reconnect: typed once.
    assert_eq!(fs::read_to_string(&got).unwrap(), "手机发来的话");
}
