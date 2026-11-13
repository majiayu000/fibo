//! A long conversation a phone opens part way: older messages come when it scrolls there.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::protocol::{rpc, Request};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

const ID: &str = "00000000-0000-4000-8000-000000000001";

fn line(mut row: Value) -> String {
    row["sessionId"] = ID.into();
    row["cwd"] = "/project".into();
    row["entrypoint"] = "cli".into();
    row["userType"] = "external".into();
    row["timestamp"] = fibo_core::content::now().into();
    format!("{row}\n")
}
fn user(n: usize) -> Value {
    json!({"type":"user","uuid":format!("u{n}"),"message":{"content":format!("第 {n} 句 {}", "。".repeat(2000))}})
}
fn call(n: usize) -> Value {
    json!({"type":"assistant","uuid":format!("c{n}"),"message":{"content":[{"type":"tool_use","id":format!("t{n}"),"name":"Bash","input":{"command":format!("echo {n}")}}]}})
}
fn reply(n: usize) -> Value {
    json!({"type":"assistant","uuid":format!("a{n}"),"message":{"stop_reason":"end_turn","content":[{"type":"text","text":format!("答 {n}")}]}})
}

fn want_first() -> String {
    format!("第 0 句 {}", "。".repeat(2000))
}

fn session(h: &Host, cli: &str) -> String {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let w = h.workspace();
        if let Some(s) = w.agent_sessions.iter().find(|s| s.cli_session_id == cli) {
            return s.id.clone();
        }
        assert!(Instant::now() < until, "the walk never found {cli}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// All of it, once, in the log's order, with tools on the older part too, and `partial` stops
/// saying so only at the start. A view-only phone can do this: it changes nothing on the machine.
#[test]
fn a_phone_reads_older_messages_back_when_it_gets_there() {
    let turns = 400;
    let h = Host::start_over(|root| {
        let dir = root.join("logs/claude/project");
        fs::create_dir_all(&dir).unwrap();
        let mut log = String::new();
        for n in 0..turns {
            log += &line(user(n));
            log += &line(call(n));
            log += &line(reply(n));
        }
        fs::write(dir.join(format!("{ID}.jsonl")), log).unwrap();
    });
    let s = session(&h, ID);
    let mut r = Remote::open(&h, Some("view"), &[]);
    assert!(r.ops().contains(&"backfill".to_owned()));
    let first = r.ok(json!({ "op": "chat", "session": s, "limit": 500 }));
    assert_eq!(first["partial"], true);
    let at_first = first["messages"].as_array().unwrap().clone();
    assert!(at_first.len() < turns, "{}", at_first.len());
    // Not the first ask kept from the log's head: paging back from it would skip what follows.
    assert_ne!(at_first[0]["text"], want_first());
    let (mut calls, mut found) = (0, 0);
    loop {
        let got = r.ok(json!({ "op": "backfill", "session": s }));
        found += got["added"].as_u64().unwrap();
        calls += 1;
        if got["done"] == true {
            break;
        }
        assert!(calls < 100, "never done");
    }
    assert!(found > 0);
    // Paged back as a phone does, from what it already had.
    let mut before = at_first[0]["seq"].as_i64().unwrap();
    let mut older = vec![];
    loop {
        let page = r.ok(
            json!({ "op": "chat", "session": s, "before": before, "limit": 50, "tools": true }),
        );
        let mut got = page["messages"].as_array().unwrap().clone();
        before = got[0]["seq"].as_i64().unwrap();
        got.append(&mut older);
        older = got;
        if page["more"] == false {
            assert_eq!(page["partial"], Value::Null, "read to its start: {page}");
            break;
        }
    }
    older.extend(at_first);
    let text = |m: &Value| {
        m["text"]
            .as_str()
            .unwrap()
            .trim_end_matches([' ', '。'])
            .to_owned()
    };
    let want: Vec<String> = (0..turns)
        .flat_map(|n| [format!("第 {n} 句"), format!("答 {n}")])
        .collect();
    assert_eq!(older.iter().map(text).collect::<Vec<_>>(), want);
    let seqs: Vec<i64> = older.iter().map(|m| m["seq"].as_i64().unwrap()).collect();
    assert!(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "seq grows along the log"
    );
    assert!(
        seqs.iter().all(|s| *s < (1i64 << 53)),
        "a phone's numbers hold it"
    );
    // The oldest stretch: its tool count, and its calls can be opened.
    assert_eq!(older[1]["tools"]["commands"], 1, "{}", older[1]);
    let list = r.ok(json!({ "op": "toolcalls", "session": s, "seq": seqs[1] }));
    assert_eq!(list["calls"].as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list["calls"][0]["id"], "t0");
    // Asked again, nothing more and no error.
    let again = r.ok(json!({ "op": "backfill", "session": s }));
    assert_eq!(again, json!({ "added": 0, "done": true }));
    // Starts counting new lines from where the log is, unaffected.
    let tail = r.ok(json!({ "op": "chat", "session": s, "after": seqs[seqs.len() - 2] }));
    assert_eq!(tail["messages"].as_array().unwrap().len(), 1);
}

/// `MOORING_TOOLS_LOG=<a long real log> cargo test --release -p fibo-host --test
/// remote_backfill -- --ignored --nocapture`: a clone of the log, read back beside a host that
/// is answering; prints timings only.
#[test]
#[ignore = "reads a real log; prints timings only"]
fn real_backfill_costs() {
    let source = std::env::var("MOORING_TOOLS_LOG").expect("MOORING_TOOLS_LOG");
    let source = Path::new(&source).to_owned();
    let stem = source.file_stem().unwrap().to_string_lossy().into_owned();
    let (from, name) = (source.clone(), source.file_name().unwrap().to_owned());
    let h = Host::start_over(|root| {
        let dir = root.join("logs/claude/project");
        fs::create_dir_all(&dir).unwrap();
        // A clone: no bytes copied, and the original is never opened for writing.
        let done = std::process::Command::new("/bin/cp")
            .arg("-c")
            .arg(&from)
            .arg(dir.join(&name))
            .status()
            .unwrap();
        assert!(done.success());
    });
    let s = session(&h, &stem);
    h.launch("t", Path::new("/tmp"), "while :; do date; sleep 0.2; done");
    let mut r = Remote::open(&h, None, &[]);
    let probe = |secs: f64| -> std::thread::JoinHandle<Vec<f64>> {
        let socket = h.socket();
        std::thread::spawn(move || {
            let (until, mut took) = (Instant::now() + Duration::from_secs_f64(secs), Vec::new());
            while Instant::now() < until {
                let t = Instant::now();
                let _ = rpc(
                    &socket,
                    &Request::ReadScreen {
                        id: "t".into(),
                        rows: 5,
                    },
                );
                took.push(t.elapsed().as_secs_f64() * 1000.0);
                std::thread::sleep(Duration::from_millis(5));
            }
            took
        })
    };
    let print = |name: &str, mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        let at = |q: f64| v[((v.len() as f64 * q) as usize).min(v.len() - 1)];
        println!(
            "{name}: {} replies, p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
            v.len(),
            at(0.5),
            at(0.99),
            at(1.0)
        );
    };
    print("host alone", probe(3.0).join().unwrap());
    let first = r.ok(json!({ "op": "chat", "session": s, "limit": 500 }));
    println!(
        "{} MB, first read: {} messages, partial {}",
        fs::metadata(&source).unwrap().len() >> 20,
        first["messages"].as_array().unwrap().len(),
        first["partial"]
    );
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let beside = {
        let (socket, stop) = (h.socket(), stop.clone());
        std::thread::spawn(move || {
            let mut took = Vec::new();
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let t = Instant::now();
                let _ = rpc(
                    &socket,
                    &Request::ReadScreen {
                        id: "t".into(),
                        rows: 5,
                    },
                );
                took.push(t.elapsed().as_secs_f64() * 1000.0);
                std::thread::sleep(Duration::from_millis(5));
            }
            took
        })
    };
    let (began, mut slowest, mut calls, mut found, mut chats) =
        (Instant::now(), 0f64, 0, 0, Vec::new());
    loop {
        let t = Instant::now();
        let got = r.ok(json!({ "op": "backfill", "session": s }));
        slowest = slowest.max(t.elapsed().as_secs_f64() * 1000.0);
        calls += 1;
        found += got["added"].as_u64().unwrap();
        let t = Instant::now();
        r.ok(json!({ "op": "chat", "session": s, "limit": 50, "tools": true }));
        chats.push(t.elapsed().as_secs_f64() * 1000.0);
        if got["done"] == true {
            break;
        }
    }
    println!(
        "read back in {:.1} s, {calls} calls, slowest {slowest:.0} ms, {found} messages",
        began.elapsed().as_secs_f64()
    );
    print("chat beside it", chats);
    let last = r.ok(json!({ "op": "chat", "session": s, "limit": 1 }));
    println!("partial after: {}", last["partial"]);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    print("host beside it", beside.join().unwrap());
}
