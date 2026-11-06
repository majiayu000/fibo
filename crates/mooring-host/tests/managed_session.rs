#[path = "support/tied.rs"]
mod tied;
use mooring_core::{model::*, protocol::*, terminal::Vt};
use std::{
    fs, io,
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::{Duration, Instant},
};
use tied::SpawnTied;

#[path = "managed_session/claude_logs.rs"]
mod claude_logs;
#[path = "managed_session/cli.rs"]
mod cli;
#[path = "managed_session/codex_daemon.rs"]
mod codex_daemon;
#[path = "managed_session/codex_fixture.rs"]
mod codex_fixture;
#[path = "managed_session/codex_rebind.rs"]
mod codex_rebind;
#[path = "managed_session/common.rs"]
mod common;
#[path = "managed_session/events.rs"]
mod events;
#[path = "managed_session/hooks.rs"]
mod hooks;
#[path = "managed_session/lifecycle.rs"]
mod lifecycle;
#[path = "managed_session/ownership.rs"]
mod ownership;
#[path = "managed_session/phone.rs"]
mod phone;
#[path = "managed_session/phone_fixture.rs"]
mod phone_fixture;
#[path = "managed_session/relay.rs"]
mod relay;
#[path = "managed_session/remote.rs"]
mod remote;
#[path = "managed_session/resources.rs"]
mod resources;
#[path = "managed_session/restart_binding.rs"]
mod restart_binding;
#[path = "managed_session/status.rs"]
mod status;
#[path = "managed_session/terminal_io.rs"]
mod terminal_io;
#[path = "managed_session/todo.rs"]
mod todo;

use codex_fixture::*;
use common::*;
use phone_fixture::*;

#[test]
#[ignore = "subprocess daemon entry for integration fixtures"]
fn daemon_fixture() {
    let root = PathBuf::from(
        std::env::var_os("MOORING_FIXTURE_RUNTIME").expect("fixture subprocess only"),
    );
    // Roots do not need to exist; production must also handle an uninstalled CLI.
    let roots = vec![
        (Agent::Codex, root.join("logs/codex")),
        (Agent::Claude, root.join("logs/claude")),
        (Agent::Grok, root.join("grok-home/sessions")),
    ];
    if std::env::var_os("MOORING_FIXTURE_TAKEOVER").is_some() {
        mooring_host::serve_taking_over(&root, roots, 7).unwrap();
    } else {
        mooring_host::serve_with_roots(&root, roots).unwrap();
    }
}

/// Not a check: a live machine for trying the phone app by hand. Starts a host, a relay
/// on 127.0.0.1:8790, an Agent that answers whatever it is sent and one stopped on an
/// approval, prints a pairing link, and runs until killed:
/// `cargo test -p mooring-host --test managed_session -- --ignored --exact phone_demo_machine --nocapture`
/// (prefix `MOORING_DEMO_LAN=<Wi-Fi address>` to pair a real phone on the same network)
#[test]
#[ignore = "manual: a demo machine for the phone app"]
fn phone_demo_machine() {
    use serde_json::json;
    let h = Host::start();
    h.setup();
    let logs = h.root.join("logs/claude");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("claude");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    let start = |name: &str, first: &str, said: &str, script: &dyn Fn(&str, &str) -> String| {
        let id = uuid::Uuid::new_v4().to_string();
        let log = logs.join(format!("{id}.jsonl"));
        let at = mooring_core::content::now();
        let user = json!({"type":"user","sessionId":id,"uuid":"u0","cwd":"/Users/demo/mooring","entrypoint":"cli","userType":"external","timestamp":at,"message":{"content":first}});
        let agent = json!({"type":"assistant","sessionId":id,"uuid":"a0","timestamp":at,"message":{"content":[{"type":"text","text":said}]}});
        let seed = h.root.join(format!("seed-{name}"));
        fs::write(&seed, format!("{user}\n{agent}\n")).unwrap();
        let body = script(&id, &log.to_string_lossy());
        let script = format!(
            "cat {} > {}; {body}",
            shell_quote(&seed.to_string_lossy()),
            shell_quote(&log.to_string_lossy())
        );
        h.call(h.launch(
            name,
            &format!(
                "exec {} -c {} fixture --session-id {}",
                shell_quote(&cli.to_string_lossy()),
                shell_quote(&script),
                id
            ),
        ))
        .unwrap();
    };
    // Each line typed in comes back as the user's message and a reply, as Claude logs them.
    let echo = |id: &str, log: &str| {
        let py = format!(
            "import json,sys,time,uuid; t=time.strftime('%Y-%m-%dT%H:%M:%S.000Z',time.gmtime()); l=sys.argv[1]; \
             print(json.dumps({{'type':'user','sessionId':'{id}','uuid':str(uuid.uuid4()),'cwd':'/Users/demo/mooring','entrypoint':'cli','userType':'external','timestamp':t,'message':{{'content':l}}}},ensure_ascii=False)); \
             print(json.dumps({{'type':'assistant','sessionId':'{id}','uuid':str(uuid.uuid4()),'timestamp':t,'message':{{'content':[{{'type':'text','text':'收到：'+l+'。我先看一下相关代码，再告诉你结论。'}}]}}}},ensure_ascii=False))"
        );
        format!(
            "while read -r line; do sleep 1; python3 -c {} \"$line\" >> {}; done",
            shell_quote(&py),
            shell_quote(log)
        )
    };
    start(
        "chat",
        "看看手机端的聊天接口还差什么",
        "后台的 chat 操作已经能按终端取对话了。还差两件事：\n1. 手机端轮询新消息\n2. 断线重连后补齐中间的消息\n\n要我先做哪一件？",
        &echo,
    );
    start(
        "approve",
        "把 main 推到远端",
        "本地检查都过了，准备推送。",
        &|_, _| {
            "sleep 2; printf '%s\\n' 'Bash command' '  git push origin main' 'Do you want to proceed?' '> 1. Yes' '  2. No'; read -r answer; printf '\\033[2J\\033[Hpushed\\n'; read -r end".into()
        },
    );
    h.call(h.launch("shell", "exec cat")).unwrap();

    // MOORING_DEMO_LAN=<this Mac's Wi-Fi address> listens on every interface, so a real
    // phone on the same network can pair; otherwise only this machine and its simulators.
    let lan = std::env::var("MOORING_DEMO_LAN").ok();
    let listen = if lan.is_some() {
        "0.0.0.0:8790"
    } else {
        "127.0.0.1:8790"
    };
    let (bound, _stop, _relay) = local_relay(listen, Default::default());
    let base = lan.map_or(bound, |ip| format!("ws://{ip}:8790"));
    let root = h.root.to_string_lossy().into_owned();
    let link = pair(&root, &base, "演示手机", "full");
    let mut machine = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--relay", &base, "--runtime", &root])
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    println!("PAIRING LINK: {}", link.text());
    println!("RUNTIME: {root}");
    machine.wait().ok();
}
