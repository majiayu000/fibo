use super::*;
#[test]
fn notifications_only_follow_new_attention_facts_and_opt_in() {
    let mut w = Workspace::default();
    // A conversation live in a Mooring terminal open in a tab: the only kind that
    // asks for you (`status::needs_you`).
    w.spaces.push(Space {
        id: "space".into(),
        name: "工作空间".into(),
        cwd: "/project".into(),
    });
    w.terminals.push(Terminal {
        id: "pty".into(),
        space: "space".into(),
        argv: vec![],
        cwd: "/project".into(),
        pid: None,
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    w.tabs.push(Tab {
        id: "tab".into(),
        space: "space".into(),
        default_task: None,
        title: String::new(),
        layout: Some(Layout::Leaf("pty".into())),
    });
    let s = AgentSession {
        id: "s".into(),
        runtime: Agent::Codex,
        cli_session_id: "cli".into(),
        terminal: Some("pty".into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: now(),
        cwd: "/project".into(),
        transcript: "log".into(),
        binding: None,
        name: String::new(),
    };
    w.agent_sessions.push(s);
    let before = w.clone();
    w.content.insert(
        "s".into(),
        mooring_core::content::Content {
            interactive: true,
            turn: mooring_core::content::Turn::Waiting,
            waiting: "选哪一个？ A / B".into(),
            updated: now(),
            turn_started: now(),
            ..Default::default()
        },
    );
    assert!(entered(&before, &w, None).is_empty());
    // The sound alone is reason enough to report it.
    w.settings.sound = true;
    assert_eq!(entered(&before, &w, None).len(), 1);
    w.settings.sound = false;
    w.settings.notifications = true;
    assert_eq!(entered(&before, &w, None).len(), 1);
    assert!(entered(&w, &w, None).is_empty());
    let previous = w.clone();
    w.content.get_mut("s").unwrap().waiting = "选哪一个？（Hook 补充的说明）".into();
    assert!(
        entered(&previous, &w, None).is_empty(),
        "a second signal for the same dialog is not news"
    );
    let c = w.content.get_mut("s").unwrap();
    c.waiting = "是否执行测试？".into();
    c.waiting_episode += 1;
    assert_eq!(entered(&previous, &w, None).len(), 1);
    assert!(
        entered(&previous, &w, Some("pty")).is_empty(),
        "no banner for the pane the user is looking at"
    );
    assert_eq!(entered(&previous, &w, Some("other")).len(), 1);
    w.agent_sessions[0].outcome = Outcome::Completed;
    assert!(entered(&previous, &w, None).is_empty());
    let value: serde_json::Value =
        serde_json::from_str(&panel(&previous, &Link::fresh(RecognitionPhase::Healthy))).unwrap();
    assert_eq!(value["rows"].as_array().unwrap().len(), 1);
    // Banners and the menu panel use the name the sidebar shows, not the raw first ask.
    let mut renamed = previous.clone();
    renamed.agent_sessions[0].name = "发布前检查".into();
    let value: serde_json::Value =
        serde_json::from_str(&panel(&renamed, &Link::fresh(RecognitionPhase::Healthy))).unwrap();
    assert_eq!(value["rows"][0]["title"], "发布前检查");
    let mut asked = renamed.clone();
    let c = asked.content.get_mut("s").unwrap();
    c.waiting = "要继续吗？".into();
    c.waiting_episode += 1;
    assert_eq!(
        entered(&renamed, &asked, None)[0].2,
        "工作空间 · 发布前检查"
    );
    let mut scanned = previous.clone();
    scanned.agent_sessions[0].terminal = None;
    assert!(
        entered(&before, &scanned, None).is_empty(),
        "a session Fibo did not start never asks for attention"
    );
    let value: serde_json::Value =
        serde_json::from_str(&panel(&scanned, &Link::fresh(RecognitionPhase::Healthy))).unwrap();
    assert!(value["rows"].as_array().unwrap().is_empty());
    assert_eq!(value["badge"], "");
}
/// A managed session in space "工作空间" whose content is `turn`, updated `ago` seconds ago.
fn session(w: &mut Workspace, id: &str, turn: mooring_core::content::Turn, ago: u64) {
    use mooring_core::content::{Content, Turn};
    if w.spaces.is_empty() {
        w.spaces.push(Space {
            id: "space".into(),
            name: "工作空间".into(),
            cwd: "/project".into(),
        });
    }
    w.terminals.push(Terminal {
        id: format!("pty-{id}"),
        space: "space".into(),
        argv: vec![],
        cwd: "/project".into(),
        pid: None,
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    });
    // Open in a tab, as every terminal that can ask for you is.
    w.tabs.push(Tab {
        id: format!("tab-{id}"),
        space: "space".into(),
        default_task: None,
        title: String::new(),
        layout: Some(Layout::Leaf(format!("pty-{id}"))),
    });
    w.agent_sessions.push(AgentSession {
        id: id.into(),
        runtime: Agent::Claude,
        cli_session_id: format!("cli-{id}"),
        terminal: Some(format!("pty-{id}")),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: now() - ago,
        cwd: "/project".into(),
        transcript: "log".into(),
        binding: None,
        name: format!("会话 {id}"),
    });
    w.content.insert(
        id.into(),
        Content {
            interactive: true,
            turn,
            waiting: if turn == Turn::Waiting {
                format!("{id} 问你：继续吗？")
            } else {
                String::new()
            },
            error: if turn == Turn::Error {
                "exit 255".into()
            } else {
                String::new()
            },
            updated: now() - ago,
            turn_started: now() - ago,
            ..Default::default()
        },
    );
}
fn value(w: &Workspace, link: &Link) -> serde_json::Value {
    serde_json::from_str(&panel(w, link)).unwrap()
}
/// The three states of menubar-panel.html: some need you, none do, the host is gone.
fn samples() -> Vec<(&'static str, Workspace, Link)> {
    use mooring_core::content::Turn;
    let mut needs = Workspace::default();
    needs.settings.notifications = true;
    session(&mut needs, "a", Turn::Waiting, 120);
    session(&mut needs, "b", Turn::Waiting, 48 * 60);
    session(&mut needs, "c", Turn::Error, 2 * 3600);
    session(&mut needs, "d", Turn::Running, 30);
    let mut none = Workspace::default();
    none.settings.notifications = true;
    session(&mut none, "d", Turn::Running, 30);
    session(&mut none, "e", Turn::Running, 60);
    let offline = Link {
        phase: RecognitionPhase::Interrupted,
        error: Some("Connection refused (os error 61)".into()),
        since: now() - 1800,
    };
    let mut many = needs.clone();
    for i in 0..4 {
        session(&mut many, &format!("m{i}"), Turn::Waiting, 300 * (i + 1));
    }
    vec![
        (
            "needs",
            needs.clone(),
            Link::fresh(RecognitionPhase::Healthy),
        ),
        ("empty", none, Link::fresh(RecognitionPhase::Healthy)),
        ("offline", needs, offline),
        ("many", many, Link::fresh(RecognitionPhase::Healthy)),
    ]
}
#[test]
fn panel_counts_rows_and_says_where_each_one_runs() {
    let [(_, needs, link), ..] = &samples()[..] else {
        unreachable!()
    };
    let v = value(needs, link);
    assert_eq!(v["heading"], "3 项要你处理");
    assert_eq!(v["when"], "刚刚更新");
    assert_eq!(v["badge"], "3");
    assert_eq!(v["needs"], 3);
    let counts = &v["counts"];
    assert_eq!(
        (
            &counts["waiting"],
            &counts["errors"],
            &counts["running"],
            &counts["review"]
        ),
        (&2.into(), &1.into(), &1.into(), &0.into()),
        "numbers, not phrases the panel has to parse"
    );
    let rows = v["rows"].as_array().unwrap();
    let order: Vec<_> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(order, ["a", "b", "c"], "waiting first, newest first");
    assert_eq!(rows[0]["kind"], "wait");
    assert_eq!(rows[2]["kind"], "error");
    assert_eq!(rows[0]["meta"], "工作空间 · Claude Code");
    assert_eq!(rows[1]["age"], "48 分钟");
    assert_eq!(v["more"], 0);
    assert!(v["empty"].is_null());
    assert_eq!(v["connection"]["phase"], "healthy");
    assert!(v["connection"]["error"].is_null());
    assert_eq!(v["foot_count"], 0);
    assert_eq!(v["tokens"]["dark"]["accent"], T::menu_dark().accent);
}
#[test]
fn panel_lists_five_and_counts_the_rest() {
    use mooring_core::content::Turn;
    let mut w = Workspace::default();
    for i in 0..7 {
        session(&mut w, &format!("s{i}"), Turn::Waiting, 60 * (i + 1));
    }
    let v = value(&w, &Link::fresh(RecognitionPhase::Healthy));
    assert_eq!(v["needs"], 7);
    assert_eq!(v["rows"].as_array().unwrap().len(), 5);
    assert_eq!(v["more"], 2);
}
#[test]
fn empty_panel_says_whether_notifications_will_tell_you() {
    let samples = samples();
    let (_, none, link) = &samples[1];
    let v = value(none, link);
    assert_eq!(v["heading"], "没有要你处理的");
    assert_eq!(v["badge"], "");
    assert!(v["rows"].as_array().unwrap().is_empty());
    assert_eq!(v["empty"]["title"], "在跑的 2 个停下来要你时，会发系统通知");
    assert_eq!(v["empty"]["note"], "通知已开 · 你正看着的画面不会再弹");
    let mut quiet = none.clone();
    quiet.settings.notifications = false;
    let v = value(&quiet, link);
    assert_eq!(v["empty"]["note"], "通知没开 · 在设置的通知页打开");
    assert!(!v["empty"]["title"].as_str().unwrap().contains("系统通知"));
}
#[test]
fn offline_panel_keeps_the_old_rows_and_dates_them() {
    let samples = samples();
    let (_, needs, link) = &samples[2];
    let v = value(needs, link);
    let stopped = minute(link.since);
    assert_eq!(v["heading"], "连不上后台");
    assert_eq!(v["when"], format!("数据停在 {stopped}"));
    assert_eq!(v["section"], format!("{stopped} 时要你处理"));
    assert_eq!(v["section_note"], "可能已经变了");
    assert_eq!(v["badge"], "", "the offline badge carries no count");
    assert_eq!(v["rows"].as_array().unwrap().len(), 3);
    let c = &v["connection"];
    assert_eq!(c["phase"], "interrupted");
    assert_eq!(c["error"], "Connection refused (os error 61)");
    assert_eq!(c["since"], link.since);
    assert_eq!(v["foot_note"], "通知暂停");
    assert!(v["foot_count"].is_null());
    assert!(
        v["empty"].is_null(),
        "no reassurance while the numbers are old"
    );
}
/// Writes each sample's panel JSON for a native snapshot of the AppKit panel:
/// `MOORING_PANEL_SAMPLES=dir cargo test -p mooring-app panel_samples -- --ignored`.
#[test]
#[ignore]
fn panel_samples() {
    let dir = std::path::PathBuf::from(std::env::var_os("MOORING_PANEL_SAMPLES").unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    for (name, w, link) in samples() {
        std::fs::write(dir.join(format!("{name}.json")), panel(&w, &link)).unwrap();
    }
}

/// orca#4496: once a session no longer needs you, its banner is taken back.
#[test]
fn a_banner_goes_once_its_session_no_longer_needs_you() {
    use mooring_core::content::Turn;
    let mut before = Workspace::default();
    session(&mut before, "answered", Turn::Waiting, 60);
    session(&mut before, "still", Turn::Waiting, 60);
    session(&mut before, "closed", Turn::Waiting, 60);
    session(&mut before, "busy", Turn::Running, 60);
    let mut after = before.clone();
    after.content.get_mut("answered").unwrap().turn = Turn::Running;
    after.agent_sessions.retain(|s| s.id != "closed");
    let mut gone = left(&before, &after);
    gone.sort();
    assert_eq!(gone, ["answered", "closed"]);
    assert!(left(&after, &after).is_empty());
}
/// Nothing a space put aside does calls you, its banners go, and taking it back is not news.
#[test]
fn a_space_put_aside_stays_silent_and_taking_it_back_sends_no_burst() {
    use mooring_core::content::Turn;
    let mut w = Workspace::default();
    w.settings.notifications = true;
    session(&mut w, "asks", Turn::Waiting, 60);
    w.spaces.push(Space {
        id: "other".into(),
        name: "另一个".into(),
        cwd: "/other".into(),
    });
    let mut idle = w.clone();
    idle.content.get_mut("asks").unwrap().turn = Turn::Running;
    idle.content.get_mut("asks").unwrap().waiting.clear();
    assert_eq!(entered(&idle, &w, None).len(), 1, "it would call you");

    let mut quiet_idle = idle.clone();
    quiet_idle.quiet_spaces.insert("space".into());
    let mut quiet = w.clone();
    quiet.quiet_spaces.insert("space".into());
    assert!(
        entered(&quiet_idle, &quiet, None).is_empty(),
        "no banner while aside"
    );
    assert_eq!(left(&w, &quiet), ["asks"], "the banner already shown goes");
    let value: serde_json::Value =
        serde_json::from_str(&panel(&quiet, &Link::fresh(RecognitionPhase::Healthy))).unwrap();
    assert!(value["rows"].as_array().unwrap().is_empty());
    assert_eq!(value["badge"], "");

    assert!(
        entered(&quiet, &w, None).is_empty(),
        "no burst on the way back"
    );
    let value: serde_json::Value =
        serde_json::from_str(&panel(&w, &Link::fresh(RecognitionPhase::Healthy))).unwrap();
    assert_eq!(
        value["rows"].as_array().unwrap().len(),
        1,
        "it counts again"
    );
    // Its next new fact is a banner again.
    let mut next = w.clone();
    next.content.get_mut("asks").unwrap().waiting_episode += 1;
    assert_eq!(entered(&w, &next, None).len(), 1);
}
