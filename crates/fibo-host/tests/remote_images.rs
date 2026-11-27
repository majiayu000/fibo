//! Pictures from the phone through `fibo remote --stdio`: `image` keeps one on this
//! machine, and `send` pastes each kept path on its own before the words, as a dropped file
//! is pasted, so the Agent CLIs take it as an attachment and not as text.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use serde_json::json;
use std::fs;
use std::time::{Duration, Instant};

/// The 8 bytes every PNG starts with, as base64.
const PNG: &str = "iVBORw0KGgo=";

#[test]
fn each_picture_goes_in_alone_before_the_words_and_once_when_sent_again() {
    let h = Host::start();
    let dir = scratch(&h, "proj");
    let typed = dir.join("typed.txt");
    h.launch(
        "t",
        &dir,
        &format!("stty -echo; cat >> '{}'", typed.display()),
    );
    std::thread::sleep(Duration::from_millis(300));
    let mut r = Remote::open(&h, None, &[]);
    assert!(r.ops().contains(&"image".to_owned()));
    let a = r.ok(json!({ "op": "image", "data": PNG }))["path"]
        .as_str()
        .unwrap()
        .to_owned();
    let b = r.ok(json!({ "op": "image", "data": PNG }))["path"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(a, b);
    assert!(a.starts_with(&*h.root.join("phone-images").to_string_lossy()));
    let send = json!({
        "op": "send", "terminal": "t", "text": " 看这两张", "enter": true,
        "images": [a, b], "request": "r1",
    });
    r.ok(send.clone());
    // The reply was lost; the phone sends the same line again.
    r.ok(send);
    let until = Instant::now() + Duration::from_secs(10);
    let line = loop {
        let text = fs::read_to_string(&typed).unwrap_or_default();
        if text.contains('\n') || Instant::now() > until {
            break text;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(line, format!("{a}{b} 看这两张\n"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(fs::read_to_string(&typed).unwrap(), line, "typed twice");
}

/// Only what `image` kept may be named, refused before anything is typed; only full control
/// may keep one; and a file that is no picture is refused.
#[test]
fn a_send_names_only_kept_pictures_and_only_full_control_uploads() {
    let h = Host::start();
    let dir = scratch(&h, "proj");
    let typed = dir.join("typed.txt");
    h.launch(
        "t",
        &dir,
        &format!("stty -echo; cat >> '{}'", typed.display()),
    );
    let mut r = Remote::open(&h, None, &[]);
    let refused = r.error(json!({
        "op": "send", "terminal": "t", "text": "x", "enter": true, "images": ["/etc/hosts"],
    }));
    assert!(refused.contains("images"), "{refused}");
    assert!(r
        .error(json!({ "op": "image", "data": "IyEvYmluL3NoCg==" }))
        .contains("PNG"));
    let mut view = Remote::open(&h, Some("answer"), &[]);
    view.error(json!({ "op": "image", "data": PNG }));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(fs::read_to_string(&typed).unwrap_or_default(), "");
}

/// An Agent that reads a pasted picture off the disk before showing it, as Claude Code does:
/// the words and Enter wait for its `[Image #1]`, or Enter submits the words alone and the
/// picture lands in the next prompt (11-19, one send in two at load 90).
const SLOW_CLI: &str = r#"
import os, sys, threading, time
log = open(sys.argv[1], "a", buffering=1)
seen = b""
started = False
def show():
    time.sleep(1.0)
    os.write(1, b"\r\n> [Image #1] ")
    log.write("picture\n")
while True:
    got = os.read(0, 4096)
    if not got:
        break
    seen += got
    if b".png" in seen and not started:
        started = True
        threading.Thread(target=show).start()
    if b"\r" in got or b"\n" in got:
        log.write("enter\n")
        break
"#;

#[test]
fn the_words_wait_until_the_cli_shows_the_picture() {
    let h = Host::start();
    let dir = scratch(&h, "proj");
    let order = dir.join("order.txt");
    let cli = dir.join("slow_cli.py");
    fs::write(&cli, SLOW_CLI).unwrap();
    h.launch(
        "t",
        &dir,
        &format!(
            "stty raw -echo; python3 '{}' '{}'",
            cli.display(),
            order.display()
        ),
    );
    std::thread::sleep(Duration::from_millis(500));
    let mut r = Remote::open(&h, None, &[]);
    let picture = r.ok(json!({ "op": "image", "data": PNG }))["path"]
        .as_str()
        .unwrap()
        .to_owned();
    r.ok(json!({
        "op": "send", "terminal": "t", "text": " what is it", "enter": true,
        "images": [picture], "request": "slow1",
    }));
    let until = Instant::now() + Duration::from_secs(10);
    let seen = loop {
        let text = fs::read_to_string(&order).unwrap_or_default();
        if text.contains("enter") || Instant::now() > until {
            break text;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(
        seen, "picture\nenter\n",
        "Enter came before the picture showed"
    );
}

/// Another computer's window has no clipboard or files there: its picture is kept here like
/// a phone's, and the path it gets back is one a phone's `send` may name too.
#[test]
fn a_picture_from_another_computers_window_is_kept_here() {
    use fibo_core::protocol::{rpc, Reply, Request};
    let h = Host::start();
    let saved = rpc(&h.socket(), &Request::SaveImage { data: PNG.into() }).unwrap();
    let Reply::Text(path) = saved else {
        panic!("{saved:?}")
    };
    assert!(path.starts_with(&*h.root.join("phone-images").to_string_lossy()));
    assert!(path.ends_with(".png"));
    assert_eq!(fs::read(&path).unwrap(), b"\x89PNG\r\n\x1a\n");
    let script = rpc(
        &h.socket(),
        &Request::SaveImage {
            data: "IyEvYmluL3No".into(),
        },
    );
    assert!(script.is_err(), "{script:?}");
}
