use super::*;

fn load(preset: &Preset) -> Result<host::Plugin, String> {
    instantiate(&compile(preset)?)
}

fn session<'a>(id: &'a str, group: &'static str) -> StateSession<'a> {
    StateSession {
        id,
        title: format!("会话 {id}"),
        agent: "codex",
        short: "AB12".into(),
        group,
        cwd: "/project",
        updated: 10,
        running_since: (group == "running").then_some(5),
        line: Some("在改 sidebar.rs".into()),
        stopped: (group == "waiting").then(|| "要不要推送".into()),
        done: (group == "review").then(|| "改好了".into()),
        needs_you: group == "waiting",
        closed: false,
        selected: false,
    }
}

#[test]
fn the_shipped_wasm_preset_sorts_sessions_into_sections() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/sidebar-status");
    let (presets, report) = mooring_core::sidebar::load(vec![dir.join("sidebar.json")]);
    assert!(report[0].result.is_ok(), "{report:?}");
    let mut plugin = load(presets.last().unwrap()).unwrap();
    let mut gone = session("z", "paused");
    gone.closed = true;
    let mut elsewhere = session("d", "paused");
    elsewhere.cwd = "/other";
    let state = State {
        sessions: vec![
            session("a", "waiting"),
            session("b", "running"),
            session("c", "review"),
            elsewhere,
            gone,
        ],
        closed_total: 9,
        now: 100,
    };
    let bytes = plugin.render(&serde_json::to_vec(&state).unwrap()).unwrap();
    let reply: Reply = serde_json::from_slice(&bytes).unwrap();
    let section = |id: &str| reply.sections.iter().find(|s| s.id == id).unwrap();
    let ids = |id: &str| -> Vec<String> {
        section(id)
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Session { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(ids("need"), ["a"]);
    assert_eq!(ids("running"), ["b"]);
    assert_eq!(ids("rest"), ["c", "d"]);
    assert_eq!(ids("closed"), ["z"]);
    assert!(matches!(
        &section("running").rows[0],
        Row::Session {
            trailing: Trailing::Elapsed,
            ..
        }
    ));
    let rest = section("rest");
    assert!(rest.folded_default);
    assert!(matches!(&rest.rows[0], Row::Note { action: Some(a), .. } if a == "toggle"));
    let folders: Vec<&str> = rest
        .rows
        .iter()
        .filter_map(|r| match r {
            Row::Folder { cwd, .. } => Some(cwd.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(folders, ["/project", "/other"]);
    // More closed than it lists: the last row points at the rest in 鸟瞰.
    assert!(matches!(
        section("closed").rows.last(),
        Some(Row::Note { action: Some(a), text }) if a == "closed" && text.contains('9')
    ));
}

#[test]
fn row_ends_are_a_few_known_words_or_a_text() {
    let row = |end: &str| {
        serde_json::from_str::<Row>(&format!(r#"{{"kind":"Session","id":"a"{end}}}"#)).map(|r| {
            match r {
                Row::Session { trailing, .. } => trailing,
                _ => unreachable!(),
            }
        })
    };
    assert_eq!(row("").unwrap(), Trailing::State);
    assert_eq!(row(r#","trailing":"elapsed""#).unwrap(), Trailing::Elapsed);
    assert_eq!(row(r#","trailing":"none""#).unwrap(), Trailing::None);
    assert_eq!(
        row(r#","trailing":{"text":"PR #12"}"#).unwrap(),
        Trailing::Text {
            text: "PR #12".into()
        }
    );
    assert!(row(r#","trailing":"clock""#).is_err());
}

#[test]
fn a_module_that_is_not_wasm_is_refused_with_a_reason() {
    let d = std::env::temp_dir().join(format!("kl-wasm-bad-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("bad.wasm"), b"not wasm").unwrap();
    std::fs::write(
        d.join("sidebar.json"),
        r#"{"api":1,"presets":[{"id":"bad","name":"坏","group":"wasm","wasm":"bad.wasm"}]}"#,
    )
    .unwrap();
    let (presets, _) = mooring_core::sidebar::load(vec![d.join("sidebar.json")]);
    let err = load(presets.last().unwrap()).err().unwrap();
    assert!(err.contains("不是可用的 wasm"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// 要你处理 has one owner: what a module is told must come from the sidebar's answer, not
/// from the state's raw bucket, or a resume not taken yet would be pinned on top for it.
#[test]
fn modules_are_told_needs_you_by_the_sidebars_own_answer() {
    let source = include_str!("sidebar_wasm.rs");
    let raw = ["let b = ", "bucket(*g)"].concat();
    assert!(!source.contains(&raw));
    assert!(source.contains(&["let b = ", "self.row_bucket(listed)"].concat()));
}

#[test]
fn a_reply_too_big_to_draw_is_refused() {
    let section = |rows: usize| Section {
        id: "s".into(),
        title: None,
        count: None,
        folded_default: false,
        rows: (0..rows)
            .map(|_| serde_json::from_str(r#"{"kind":"Note","text":"x"}"#).unwrap())
            .collect(),
    };
    assert!(
        accept(Reply {
            sections: vec![section(MAX_ROWS)]
        })
        .is_ok()
    );
    let err = accept(Reply {
        sections: vec![section(MAX_ROWS + 1)],
    })
    .err()
    .unwrap();
    assert!(err.contains("太多"), "{err}");
    let many = (0..=MAX_SECTIONS).map(|_| section(0)).collect();
    assert!(accept(Reply { sections: many }).is_err());
}

/// What one left-column draw spends on a real workspace, step by step, for the
/// "is incremental input worth it" question in specs/plugins/plugin-system.md.
/// `MOORING_WS_JSON=<Workspace JSON> cargo test --release -p mooring-app
/// sidebar_cost_on_a_real_workspace -- --ignored --nocapture`
#[test]
#[ignore]
fn sidebar_cost_on_a_real_workspace() {
    use crate::model::session_state::{live_terminal, session_space, session_status};
    use std::collections::HashSet;
    use std::time::Instant;
    let path = std::env::var("MOORING_WS_JSON").expect("MOORING_WS_JSON");
    let w: Workspace = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let space = w.spaces[0].id.clone();
    let unseen = HashSet::new();
    let wasm = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/sidebar-status/sidebar_status.wasm");
    let engine = engine().unwrap();
    let mut plugin = host::Plugin::new(
        engine,
        &host::module(engine, &std::fs::read(wasm).unwrap()).unwrap(),
    )
    .unwrap();
    let open_of =
        |s: &AgentSession| live_terminal(&w, s).is_some_and(|t| w.terminal_tab(&t.id).is_some());
    // (in_space, closed pass, open pass, state json, plugin call) per draw, in µs.
    let mut runs: Vec<[f64; 5]> = Vec::new();
    let mut sizes = (0, 0, 0, 0);
    for _ in 0..200 {
        let mut t = Instant::now();
        let lap = |t: &mut Instant| {
            let us = t.elapsed().as_secs_f64() * 1e6;
            *t = Instant::now();
            us
        };
        let in_space: Vec<&AgentSession> = w
            .agent_sessions
            .iter()
            .filter(|s| {
                let hidden = match w.content.get(&s.id) {
                    Some(c) => c.hidden_for(s).is_some(),
                    None => !(s.current && s.binding.is_some()),
                };
                !hidden && session_space(&w, s) == Some(space.as_str())
            })
            .collect();
        let a = lap(&mut t);
        let mut ended: Vec<_> = in_space
            .iter()
            .filter(|s| s.terminal.is_some() && !open_of(s))
            .map(|s| (*s, session_status(&w, &unseen, s).0))
            .collect();
        ended.sort_by_key(|(s, _)| {
            std::cmp::Reverse(w.content.get(&s.id).map_or(s.updated, |c| c.updated))
        });
        let total = ended.len();
        ended.truncate(CLOSED_SHOWN);
        let b = lap(&mut t);
        let open: Vec<_> = in_space
            .iter()
            .filter(|s| open_of(s))
            .map(|s| (*s, session_status(&w, &unseen, s).0))
            .collect();
        let c = lap(&mut t);
        let state = State {
            sessions: open
                .iter()
                .map(|l| (l, false))
                .chain(ended.iter().map(|l| (l, true)))
                .map(|((s, g), closed)| {
                    let content = w.content.get(&s.id);
                    let mut row = session(&s.id, group_name(*g));
                    row.title = content.map(|c| c.title.clone()).unwrap_or_default();
                    row.short = short_id(&s.cli_session_id);
                    row.cwd = &s.cwd;
                    row.line = content.map(|c| mooring_core::content::short(&c.headline(), 100));
                    row.done = content.map(|c| c.last_reply.chars().take(80).collect());
                    row.closed = closed;
                    row
                })
                .collect(),
            closed_total: total,
            now: 0,
        };
        let bytes = serde_json::to_vec(&state).unwrap();
        let d = lap(&mut t);
        let reply = plugin.render(&bytes).unwrap();
        let _: Reply = serde_json::from_slice(&reply).unwrap();
        let e = lap(&mut t);
        sizes = (in_space.len(), total, open.len(), bytes.len());
        runs.push([a, b, c, d, e]);
    }
    let names = [
        "筛本空间",
        "已关闭全量算状态+排序",
        "打开的算状态",
        "生成输入 JSON",
        "调插件+解析回复",
    ];
    eprintln!(
        "本空间 {} 个会话，已关闭 {} 个，打开 {} 个，输入 {} 字节",
        sizes.0, sizes.1, sizes.2, sizes.3
    );
    let mut totals: Vec<f64> = runs.iter().map(|r| r.iter().sum()).collect();
    for (i, name) in names.iter().enumerate() {
        let mut v: Vec<f64> = runs.iter().map(|r| r[i]).collect();
        v.sort_by(f64::total_cmp);
        eprintln!("{name}: p50 {:.0} µs, p95 {:.0} µs", v[100], v[190]);
    }
    totals.sort_by(f64::total_cmp);
    eprintln!("合计: p50 {:.0} µs, p95 {:.0} µs", totals[100], totals[190]);
}

/// The words a module can see are the table in specs/plugins/plugin-system.md; a new
/// one belongs there too.
#[test]
fn modules_see_only_the_documented_group_words() {
    let all = [
        Status::Waiting,
        Status::Error,
        Status::Review,
        Status::Paused,
        Status::Running,
        Status::Stale,
        Status::Running,
        Status::Review,
        Status::Review,
        Status::Fresh,
        Status::Background,
        Status::Closed,
    ];
    let documented = [
        "waiting", "error", "running", "review", "paused", "fresh", "stale",
    ];
    for g in all {
        assert!(documented.contains(&group_name(g)), "{}", group_name(g));
        assert_eq!(
            super::super::sidebar_list::bucket(g) == Bucket::Attention,
            matches!(group_name(g), "waiting" | "error" | "review")
        );
    }
}
