//! Pictures from the phone through `mooring remote --stdio`: `image` keeps one on this
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
