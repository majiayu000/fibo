//! A terminal tied to the conversation the host started its Agent with (`Launch.start`),
//! as a phone's 新任务, 接着做 or a todo's 开始 starts one: the task on the command line, so no
//! Enter, and drawn by the TUI its own way. Undoing the start evidence leaves every one of
//! these unbound, as RC3 was on the phone (2026-11-17 walk).
use super::*;
use std::path::Path;

/// A shared-daemon Codex in `pane`, started with `prompt` on its command line. Its TUI draws
/// `shown` (the task as Codex draws it: `code` without its backticks) and sees no Enter for
/// it; a process that is not Codex holds the thread's log open, as the daemon does, and
/// appends the turn the task opened once `gate` exists (at once without one), logged two
/// seconds after this call: never before the host started the pane. Each line typed into
/// it afterwards is the thread's next prompt, logged as it is read.
fn started_codex(
    h: &Host,
    pane: &str,
    thread: &str,
    (prompt, shown): (&str, &str),
    gate: Option<&Path>,
    start: Option<AgentStart>,
) {
    fake_codex(h, pane, thread, prompt, Some(shown), gate, start);
}

/// `started_codex`, but the TUI echoes nothing typed into it and never draws its task: a real
/// one draws its own input box and scrolls the prompt away under the Agent's output, so a
/// prompt typed there leaves only its Enter.
fn quiet_codex(
    h: &Host,
    pane: &str,
    thread: &str,
    prompt: &str,
    gate: Option<&Path>,
    start: Option<AgentStart>,
) {
    fake_codex(h, pane, thread, prompt, None, gate, start);
}

/// The fake behind both: an empty line typed (Enter for an approval, say) logs nothing, and
/// `/quit` ends it.
fn fake_codex(
    h: &Host,
    pane: &str,
    thread: &str,
    prompt: &str,
    shown: Option<&str>,
    gate: Option<&Path>,
    start: Option<AgentStart>,
) {
    let logs = h.root.join("logs/codex");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("codex");
    if !cli.exists() {
        std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    }
    let q = |p: &Path| shell_quote(&p.to_string_lossy());
    let fifo = h.root.join(format!("{pane}-fifo"));
    assert!(Command::new("/usr/bin/mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let at = fibo_core::content::now() + 2;
    let meta = h.root.join(format!("{pane}-meta"));
    let meta_row = serde_json::json!({"type":"session_meta","payload":{"id":thread,"cwd":"/tmp"}});
    fs::write(&meta, format!("{meta_row}\n")).unwrap();
    let turn = h.root.join(format!("{pane}-turn"));
    fs::write(
        &turn,
        format!(
            "{}\n{}\n",
            serde_json::json!({"timestamp":at,"type":"event_msg","payload":{"type":"task_started","turn_id":format!("{pane}-1")}}),
            serde_json::json!({"timestamp":at,"type":"event_msg","payload":{"type":"user_message","message":prompt}})
        ),
    )
    .unwrap();
    let draw = match shown {
        Some(shown) => {
            let drawn = h.root.join(format!("{pane}-shown"));
            fs::write(&drawn, shown).unwrap();
            format!("printf '› '; cat {}; printf '\\n'; ", q(&drawn))
        }
        None => "stty -echo; printf '› working on the task\\n'; ".to_owned(),
    };
    let wait = gate.map_or(String::new(), |g| {
        format!("while [ ! -e {} ]; do sleep 0.05; done; ", q(g))
    });
    let typed = r#"while read -r line; do [ "$line" = /quit ] && exit 0; [ -z "$line" ] && continue; now=$(/bin/date +%s); printf '{"timestamp":%s,"type":"event_msg","payload":{"type":"user_message","message":"%s"}}\n' "$now" "$line" >&3; done"#;
    let script = format!(
        "/bin/cat < {fifo} >> {log} & exec 3> {fifo}; cat {meta} >&3; {draw}{wait}sleep 2; cat {turn} >&3; {typed}",
        fifo = q(&fifo),
        log = q(&logs.join(format!("{thread}.jsonl"))),
        meta = q(&meta),
        turn = q(&turn),
    );
    launch_started(
        h,
        pane,
        &format!("exec {} -c {}", q(&cli), shell_quote(&script)),
        start,
    );
}

fn launch_started(h: &Host, pane: &str, command: &str, start: Option<AgentStart>) {
    let Request::Launch {
        id,
        space,
        argv,
        cwd,
        cols,
        rows,
        scheme,
        colors,
        ..
    } = h.launch(pane, command)
    else {
        unreachable!()
    };
    h.call(Request::Launch {
        id,
        space,
        argv,
        cwd,
        cols,
        rows,
        scheme,
        colors,
        start,
    })
    .unwrap();
}

/// The task as the phone sent it in the RC3 walk, and as Codex drew it.
const TASK: (&str, &str) = (
    "This directory has two scripts. Start `sh serve.sh` and `sh job.sh` as two separate background terminals.\nReply only: started.",
    "This directory has two scripts. Start sh serve.sh and sh job.sh as two separate background\n  terminals.\n  Reply only: started.",
);

fn task_start() -> Option<AgentStart> {
    Some(AgentStart::new(Agent::Codex, Some(TASK.0.into())))
}

#[test]
fn a_task_on_the_command_line_binds_though_the_screen_redraws_it() {
    let h = Host::start();
    h.setup();
    let [a] = ids();
    started_codex(&h, "p0", &a, TASK, None, task_start());
    wait_for(&h, "the conversation its task began never bound", |w| {
        current_is(w, "p0", &a)
    });
    let w = h.workspace();
    assert_eq!(
        w.current_agent("p0").unwrap().binding,
        Some(Binding::Screen)
    );
    assert!(
        w.agent_starts.is_empty(),
        "a start ends once it has tied its terminal"
    );
}

/// Binding a pane to another's conversation is worse than none: phone chat would type into
/// the wrong Agent. Two started with one task in one folder stay unbound.
#[test]
fn two_terminals_started_with_one_task_in_one_folder_bind_neither() {
    let h = Host::start();
    h.setup();
    let [a, b] = ids();
    started_codex(&h, "p0", &a, TASK, None, task_start());
    started_codex(&h, "p1", &b, TASK, None, task_start());
    wait_for(&h, "both conversations logged their task", |w| {
        logged(w, &[&a, &b])
    });
    let until = Instant::now() + Duration::from_secs(6);
    while Instant::now() < until {
        let w = h.workspace();
        assert!(
            w.current_agent("p0").is_none() && w.current_agent("p1").is_none(),
            "a pane took a conversation two panes fit"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// What tells two such panes apart is the next prompt typed into either: Enter there sends
/// that pane's own thread on, so the pane goes by the Enter and no longer by its start, and
/// the other pane is left with the other thread. Neither ever holds the other's.
#[test]
fn two_terminals_started_alike_tell_apart_once_one_is_typed_into() {
    told_apart_by_typing(TASK);
}

/// The same when each pane draws its task verbatim, as Codex does a two-line one: each shows
/// the other's thread too, but that is the drawing of its own start, not evidence of its own.
#[test]
fn two_terminals_drawing_their_task_verbatim_tell_apart_once_one_is_typed_into() {
    told_apart_by_typing((TASK.0, TASK.0));
}

fn told_apart_by_typing(task: (&str, &str)) {
    let h = Host::start();
    h.setup();
    let [a, b] = ids();
    let start = || Some(AgentStart::new(Agent::Codex, Some(task.0.into())));
    started_codex(&h, "p0", &a, task, None, start());
    started_codex(&h, "p1", &b, task, None, start());
    wait_for(&h, "both conversations logged their task", |w| {
        logged(w, &[&a, &b])
    });
    // Typed well after both tasks were logged: Enter that close to the other thread's task
    // would be evidence for that one as well, and then nothing tells them apart.
    std::thread::sleep(Duration::from_secs(
        fibo_host::recognition::SUBMIT_WINDOW + 2,
    ));
    let w = h.workspace();
    assert!(w.current_agent("p0").is_none() && w.current_agent("p1").is_none());
    type_into(&h, "p0", "only this pane goes on: list the two scripts\r");
    wait_for(&h, "typing into one pane never told the two apart", |w| {
        assert!(
            !current_is(w, "p0", &b) && !current_is(w, "p1", &a),
            "a pane took the other pane's conversation"
        );
        current_is(w, "p0", &a) && current_is(w, "p1", &b)
    });
}

fn type_into(h: &Host, pane: &str, bytes: &str) {
    let (mut stream, _vt, writable) = h.attach(pane, true);
    assert!(writable);
    let input = Request::Input {
        bytes: bytes.as_bytes().to_vec(),
        lossy: false,
    };
    send(&mut stream, CONTROL, &input).unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
}

/// Two panes started with one task in one folder; the twin (p1) never logs its own, as Codex
/// waiting on its trust question there would not, and p0 is told apart by typing into it. Its
/// conversation stays with p0 however p0's evidence ages, and the twin, which never ran it,
/// never takes it (#403 review: it did, and the phone's chat for it typed into the twin's TUI).
/// Returns p0's conversation.
fn typed_apart(h: &Host) -> String {
    let [a, b] = ids();
    let never = h.root.join("never");
    // The twin first, so p0's task is logged after both starts.
    quiet_codex(h, "p1", &b, TASK.0, Some(&never), task_start());
    quiet_codex(h, "p0", &a, TASK.0, None, task_start());
    wait_for(h, "p0's conversation never logged its task", |w| {
        logged(w, &[&a])
    });
    std::thread::sleep(Duration::from_secs(
        fibo_host::recognition::SUBMIT_WINDOW + 2,
    ));
    type_into(h, "p0", "only this pane goes on: list the two scripts\r");
    wait_for(h, "typing into p0 never bound it", |w| {
        current_is(w, "p0", &a)
    });
    a
}

/// For a few seconds the twin never holds `a`, and p0 does when `p0_holds`.
fn twin_never_takes(h: &Host, a: &str, p0_holds: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        let w = h.workspace();
        assert!(!current_is(&w, "p1", a), "the twin took p0's conversation");
        assert!(
            !p0_holds || current_is(&w, "p0", a),
            "p0 lost its conversation"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// p0's prompt drops out of its last eight presses (approvals answered with Enter, which log
/// nothing), then another conversation's prompt and the window's 标记完成 on the twin
/// (`AssociateScreen`) compare screens again.
#[test]
fn a_conversation_told_apart_by_typing_stays_once_its_enter_is_old() {
    let h = Host::start();
    h.setup();
    let a = typed_apart(&h);
    std::thread::sleep(Duration::from_secs(
        fibo_host::recognition::SUBMIT_WINDOW + 2,
    ));
    for _ in 0..8 {
        type_into(&h, "p0", "\r");
        std::thread::sleep(Duration::from_millis(150));
    }
    // Clear of those presses: an Enter in p0 that close to p2's prompt is evidence for p0 too,
    // and p2 would bind nowhere.
    std::thread::sleep(Duration::from_secs(
        fibo_host::recognition::SUBMIT_WINDOW + 2,
    ));
    let [c] = ids();
    let other = "请检查发布脚本里的回滚步骤是否完整，然后只回答 ok";
    started_codex(&h, "p2", &c, (other, other), None, None);
    wait_for(&h, "p2 never bound by its screen", |w| {
        current_is(w, "p2", &c)
    });
    h.call(Request::AssociateScreen { id: "p1".into() })
        .unwrap();
    twin_never_takes(&h, &a, true);
}

/// A newer host takes over: the presses an older one saw are not kept.
#[test]
fn a_conversation_told_apart_by_typing_stays_after_a_takeover() {
    let mut h = Host::start();
    h.setup();
    let a = typed_apart(&h);
    let _next = super::restart_binding::take_over(&mut h);
    twin_never_takes(&h, &a, true);
}

/// p0's Agent ends: no pane holds the conversation, and the twin still never ran it.
#[test]
fn a_conversation_told_apart_by_typing_stays_off_the_twin_once_its_agent_ends() {
    let h = Host::start();
    h.setup();
    let a = typed_apart(&h);
    type_into(&h, "p0", "/quit\r");
    wait_for(&h, "p0's Agent never ended", |w| !current_is(w, "p0", &a));
    twin_never_takes(&h, &a, false);
}

/// Every one of these conversations is known and has logged its task.
fn logged(w: &Workspace, threads: &[&String]) -> bool {
    threads.iter().all(|id| {
        w.agent_sessions.iter().any(|s| {
            &&s.cli_session_id == id && w.content.get(&s.id).is_some_and(|c| !c.title.is_empty())
        })
    })
}

/// `codex resume <id>` names its conversation, but resuming writes nothing to its log until it
/// is spoken to, and no Enter comes: the host has to look for it itself, or nothing does.
#[test]
fn a_terminal_started_resuming_binds_before_it_is_spoken_to() {
    let h = Host::start();
    h.setup();
    let [a] = ids();
    let logs = h.root.join("logs/codex");
    fs::create_dir_all(&logs).unwrap();
    let rows = [
        serde_json::json!({"type":"session_meta","payload":{"id":a,"cwd":"/tmp"}}),
        serde_json::json!({"timestamp":fibo_core::content::now(),"type":"event_msg","payload":{"type":"user_message","message":"上次的任务：整理发布说明"}}),
    ];
    let lines: String = rows.iter().map(|r| format!("{r}\n")).collect();
    fs::write(logs.join(format!("{a}.jsonl")), lines).unwrap();
    wait_for(&h, "the old conversation is known", |w| {
        w.agent_sessions.iter().any(|s| s.cli_session_id == a)
    });
    let cli = h.root.join("codex");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    launch_started(
        &h,
        "resumed",
        &format!(
            "exec {} -c {} resume {a}",
            shell_quote(&cli.to_string_lossy()),
            shell_quote("printf 'RESUMED\\n'; while read -r line; do :; done")
        ),
        Some(AgentStart::resuming(Agent::Codex, &a)),
    );
    wait_for(&h, "the resumed conversation never bound", |w| {
        current_is(w, "resumed", &a)
    });
}

/// A newer host takes over a pane whose Agent the old one started (its start is in the store)
/// and one an older host launched with no start: the first binds once its task is logged, the
/// second keeps the evidence it always had (its task verbatim on the screen).
#[test]
fn a_start_outlives_a_takeover_and_a_terminal_without_one_keeps_its_evidence() {
    let mut h = Host::start();
    h.setup();
    let [a, b] = ids();
    let gate = h.root.join("gate");
    started_codex(&h, "started", &a, TASK, Some(&gate), task_start());
    let plain = "请检查发布脚本里的回滚步骤是否完整，然后只回答 ok";
    started_codex(&h, "old", &b, (plain, plain), None, None);
    wait_for(&h, "the pane from before never bound by its screen", |w| {
        current_is(w, "old", &b)
    });
    assert!(h.workspace().agent_starts.contains_key("started"));
    let _next = super::restart_binding::take_over(&mut h);
    fs::write(&gate, "").unwrap();
    wait_for(&h, "the started pane never bound after the takeover", |w| {
        current_is(w, "started", &a)
    });
    assert!(current_is(&h.workspace(), "old", &b));
}
