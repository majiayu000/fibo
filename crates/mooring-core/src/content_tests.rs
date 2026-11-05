use super::*;
#[test]
fn a_turn_its_sub_agents_keep_open_still_counts_from_its_start() {
    let mut c = Content {
        turn: Turn::Running,
        turn_started: 100,
        ..Default::default()
    };
    assert_eq!(c.running_since(), Some(100));
    c.turn = Turn::Completed;
    assert_eq!(c.running_since(), None);
    c.background = Some(Background {
        agents: 2,
        ..Default::default()
    });
    assert_eq!(c.running_since(), Some(100));
    c.turn_started = 0;
    assert_eq!(c.running_since(), None);
}
#[test]
fn a_running_turn_reads_as_words_not_a_raw_command() {
    assert_eq!(
        command_gist("S=/private/tmp/claude-502/abc/x; cd \"$S\" && FOO=1 cargo build -p mooring-app 2>&1 | tail -20").as_deref(),
        Some("cargo build -p mooring-app")
    );
    // Only assignments, and the cut a long command got when it was recorded.
    assert_eq!(
        command_gist("S=/private/tmp/claude-502/-Users-lifcc…"),
        None
    );
    assert_eq!(
        command_gist("/usr/bin/env python3 /Users/me/Library/Caches/scripts/render_all.py --out x"),
        Some("python3 render_all.py --out x".into())
    );
    assert_eq!(
        command_gist("echo ---; git status --short").as_deref(),
        Some("git status --short")
    );
    let record = |kind: &str, summary: &str, at| Evidence {
        kind: kind.into(),
        summary: summary.into(),
        exit_code: None,
        at,
        source: Source::default(),
    };
    let mut c = Content {
        turn: Turn::Running,
        turn_started: 100,
        asks: vec![Quote {
            text: "**写**主界面 A / C 两种样式".into(),
            ..Default::default()
        }],
        evidence: vec![record("command", "cargo test -p mooring-core", 50)],
        ..Default::default()
    };
    // A record from an earlier turn says nothing about this one: the ask does.
    assert_eq!(c.doing().as_deref(), Some("做 写主界面 A / C 两种样式"));
    c.evidence
        .push(record("file", "文件修改：next/crates/app/src/tabs.rs", 120));
    assert_eq!(c.doing().as_deref(), Some("改 tabs.rs"));
    c.evidence.push(record("command", "S=/private/tmp/x", 130));
    assert_eq!(c.doing().as_deref(), Some("改 tabs.rs"));
    c.evidence.push(record(
        "test",
        "cargo test -p app：test result: ok. 4 passed",
        140,
    ));
    assert_eq!(c.doing().as_deref(), Some("跑测试 cargo test -p app"));
    c.asks.clear();
    c.evidence.clear();
    assert_eq!(c.doing(), None);
}
#[test]
fn self_report_never_becomes_done() {
    let c = Content {
        last_reply: "全部测试通过，已提交 abc1234".into(),
        turn: Turn::Completed,
        completed_claim: true,
        ..Default::default()
    };
    assert!(c.here().done.contains("尚无"));
    assert!(c.here().stopped.contains("自述"));
}
#[test]
fn topic_drops_shared_lead_in() {
    assert_eq!(topic("你帮我看看 mirror 为什么挂了"), "mirror 为什么挂了");
    assert_eq!(topic("你帮我看看我最近有什么…"), "我最近有什么…");
    assert_eq!(topic("帮我看看"), "帮我看看");
    assert_eq!(topic("修复登录"), "修复登录");
    assert_eq!(
        topic("你帮我看看[Image #1] 这是哪个模型啊？"),
        "这是哪个模型啊？"
    );
    assert_eq!(topic("[Image #1][Image #2] 对比一下"), "对比一下");
    assert_eq!(topic("[Image #1]"), "[Image #1]");
    assert_eq!(topic("你帮我看看[Image #1]这是啥"), "这是啥");
    assert_eq!(topic("这是哪个模型啊？[Image #1]"), "这是哪个模型啊？");
    assert_eq!(topic("我准备 重启一下 mac"), "重启一下 mac");
    assert_eq!(topic("/goal 只要一张立绘"), "只要一张立绘");
    assert_eq!(topic("/loop"), "/loop");
    assert_eq!(
        topic("https://www.github.com/majiayu000/claude-skills?tab=1 看看这个"),
        "github.com/claude-skills 看看这个"
    );
    assert_eq!(topic("https://x.com"), "x.com");
    assert_eq!(
        topic("这是什么啊 https://x.com/someone/status/2116090187122192847"),
        "这是什么啊 x.com/someone"
    );
    assert_eq!(
        topic("https://github.com/openai/codex/issues/425 讲了什么"),
        "github.com/codex#425 讲了什么"
    );
    assert_eq!(
        topic("这里为什么报错了啊https://ultrastudio.dev/app/jobs/42"),
        "这里为什么报错了啊 ultrastudio.dev/jobs#42"
    );
    assert_eq!(
        topic("跳至主要内容\n⌘J 京东AI数字员工\n这是什么"),
        "⌘J 京东AI数字员工 这是什么"
    );
    assert_eq!(
        topic("0502c5f0b2a2480592451db00c9d4a86     480p\n帮我用dms排查一下"),
        "用dms排查一下"
    );
    assert_eq!(
        topic("PR │ dsh-desk │ 106\n这些 PR 还开着吗"),
        "这些 PR 还开着吗"
    );
    assert_eq!(topic("🔥 线上 502 了快看看"), "🔥 线上 502 了快看看");
    assert_eq!(
        topic("你帮我看看 🤔 为什么构建这么慢"),
        "🤔 为什么构建这么慢"
    );
    assert_eq!(topic("[Image #] 原样"), "[Image #] 原样");
    assert_eq!(topic("PR        | 106 |  2026…"), "PR | 106 | 2026…");
}
#[test]
fn headline_drops_the_labels_the_state_already_says() {
    let c = Content {
        last_reply: "**建议都接收**".into(),
        turn: Turn::Completed,
        ..Default::default()
    };
    assert_eq!(c.headline(), "建议都接收");
    let c = Content {
        waiting: "是否允许推送分支？".into(),
        turn: Turn::Waiting,
        ..Default::default()
    };
    assert_eq!(c.headline(), "是否允许推送分支？");
    let c = Content {
        asks: vec![Quote {
            text: "修复登录跳转".into(),
            ..Default::default()
        }],
        turn: Turn::Running,
        ..Default::default()
    };
    assert_eq!(c.headline(), "修复登录跳转");
}
