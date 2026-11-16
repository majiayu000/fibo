//! Real PTY writes through the phone endpoint: chat must never answer a menu.
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::{model::shell_quote, protocol::*};
use serde_json::json;
use std::{fs, time::Duration};

#[test]
fn approval_blocks_text_images_queue_steer_and_sendnow() {
    let h = Host::start();
    let mut phone = Remote::open(&h, None, &[]);
    for (agent, screen) in [
        (
            "claude",
            include_str!("fixtures/screens/claude/approval-bash-tip.txt"),
        ),
        (
            "codex",
            include_str!("fixtures/screens/codex/approval-cargo-test.txt"),
        ),
    ] {
        let captured = h.root.join(format!("{agent}-input"));
        let script = h.root.join(format!("{agent}.py"));
        fs::write(&script, format!(
            "import os,tty\ntty.setraw(0)\nf=open({path},'wb',buffering=0)\nprint({screen},flush=True)\nwhile True:\n b=os.read(0,1)\n f.write(b)\n if b==b'\\x1b': print('\\x1b[2J\\x1b[HREADY',flush=True)\n if b==b'\\r': print('SENT',flush=True)\n",
            path=serde_json::to_string(&captured.to_string_lossy()).unwrap(),
            screen=serde_json::to_string(&screen.replace('\n', "\r\n")).unwrap(),
        )).unwrap();
        let body = format!("python3 {}", shell_quote(&script.to_string_lossy()));
        if agent == "claude" {
            h.claude(agent, &h.root, &[json!({"type":"user","uuid":"u","cwd":h.root,"entrypoint":"cli","userType":"external","message":{"content":"phone-walk safety"}})], &body);
        } else {
            h.codex(agent, &h.root, &[], &body);
        }
        h.wait_screen(agent, "Yes");
        let session = h.workspace().current_agent(agent).unwrap().id.clone();
        let image = phone.ok(json!({"op":"image","data":"iVBORw0KGgo="}));
        for extra in [
            json!({}),
            json!({"how":"queue"}),
            json!({"how":"steer"}),
            json!({"images":[image["path"]]}),
        ] {
            // Missing optional session metadata cannot bypass the host guard.
            let mut request =
                json!({"op":"send","terminal":agent,"text":"NO-APPROVAL","enter":true});
            request
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let error = phone.error(request);
            assert!(
                error.contains("审批") || error.contains("approval"),
                "{agent}: {error}"
            );
        }
        let error = phone.error(json!({"op":"sendnow","terminal":agent}));
        assert!(
            error.contains("审批") || error.contains("approval"),
            "{error}"
        );
        assert!(h
            .call(Request::SendText {
                id: agent.into(),
                session: None,
                text: "direct".into(),
                submit: true
            })
            .is_err());
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            fs::read(&captured).unwrap(),
            b"",
            "{agent}: text/image paths must not enter the menu"
        );
        let question = phone.ok(json!({"op":"ask","terminal":agent}))["question"]
            .as_str()
            .unwrap()
            .to_owned();
        phone.ok(json!({"op":"answer","terminal":agent,"question":question,"allow":false}));
        h.wait_screen(agent, "READY");
        phone.ok(json!({"op":"send","terminal":agent,"session":session,"text":"explanation","enter":true}));
        h.wait_screen(agent, "SENT");
        assert_eq!(
            fs::read(&captured).unwrap(),
            b"\x1bexplanation\r",
            "only explicit denial and later chat reach the PTY"
        );
    }
}

#[test]
fn approval_opened_by_paste_blocks_the_later_enter() {
    let h = Host::start();
    let captured = h.root.join("race-input");
    let script = h.root.join("race.py");
    fs::write(&script, format!(
        "import os,tty\ntty.setraw(0)\nf=open({},'wb',buffering=0)\nprint('READY',flush=True)\nf.write(os.read(0,1))\nprint('\\r\\nBash command\\r\\n touch forbidden\\r\\nDo you want to proceed?\\r\\n> 1. Yes\\r\\n  2. No',flush=True)\nwhile True: f.write(os.read(0,1))\n",
        serde_json::to_string(&captured.to_string_lossy()).unwrap()
    )).unwrap();
    h.launch(
        "race",
        &h.root,
        &format!("python3 {}", shell_quote(&script.to_string_lossy())),
    );
    h.wait_screen("race", "READY");
    let error = h
        .call(Request::SendText {
            id: "race".into(),
            session: None,
            text: "x".into(),
            submit: true,
        })
        .unwrap_err();
    assert!(
        error.to_string().contains("审批") || error.to_string().contains("approval"),
        "{error}"
    );
    assert_eq!(
        fs::read(captured).unwrap(),
        b"x",
        "Enter must not follow the paste into the new approval"
    );
}

#[test]
fn mismatched_phone_version_cannot_send_after_a_failed_hello() {
    let h = Host::start();
    let mut phone = Remote::open(&h, None, &[]);
    for version in [serde_json::Value::Null, json!(1), json!(999)] {
        let error = phone.error(json!({"op":"hello", "version":version}));
        assert!(
            error.contains("更新") || error.contains("Update"),
            "{error}"
        );
        let error = phone.error(json!({"op":"send", "terminal":"unused", "text":"blocked"}));
        assert!(error.contains("hello"), "{error}");
    }
    phone.ok(json!({"op":"hello", "version":fibo_remote::protocol::VERSION}));
    phone.ok(json!({"op":"ping"}));
}
