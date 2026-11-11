use super::inspect::list_json;
use super::open::split_argv;
use super::*;
use mooring_core::agent::Agent;
fn fixture() -> Workspace {
    let mut w = Workspace::default();
    for (id, state) in [
        ("aaaa1111", ProcessState::Running),
        ("aaaa2222", ProcessState::Running),
        ("bbbb", ProcessState::Exited),
    ] {
        w.terminals.push(Terminal {
            id: id.into(),
            space: "s".into(),
            argv: vec![],
            cwd: "/tmp".into(),
            pid: None,
            state,
            exit_code: None,
            error: None,
        });
    }
    w.tabs.push(Tab {
        id: "t".into(),
        space: "s".into(),
        title: "review".into(),
        layout: Some(Layout::Leaf("aaaa2222".into())),
        default_task: None,
    });
    w.agent_sessions.push(AgentSession {
        id: "s1".into(),
        runtime: Agent::all()[0],
        cli_session_id: "c1".into(),
        terminal: Some("aaaa1111".into()),
        current: true,
        task: None,
        outcome: Outcome::Open,
        updated: 1,
        cwd: "/tmp".into(),
        transcript: "".into(),
        name: String::new(),
        binding: None,
    });
    w
}
#[test]
fn terminals_resolve_by_unique_prefix_or_tab_title_among_running_ones() {
    let w = fixture();
    assert_eq!(resolve(&w, "aaaa1").unwrap(), "aaaa1111");
    assert_eq!(resolve(&w, "review").unwrap(), "aaaa2222");
    assert!(resolve(&w, "aaaa").is_err(), "ambiguous");
    assert!(resolve(&w, "bbbb").is_err(), "exited");
}
#[test]
fn the_skill_names_the_marker_and_every_command() {
    for (lang, doc) in [("zh", SKILL), ("en", SKILL_EN)] {
        assert!(doc.contains("MOORING_ENV"), "{lang}");
        // Every command and every flag it takes, so neither language's skill can fall behind.
        let named = flags_in(doc, "");
        for c in COMMANDS {
            assert!(
                doc.contains(&format!("fibo {}", c.name)),
                "{lang}: {}",
                c.name
            );
            for f in declared(c) {
                assert!(named.contains(&f), "{lang}: {} --{f}", c.name);
            }
        }
    }
    assert!(SESSION_SKILL_EN.contains("fibo todo list"));
    assert!(!SESSION_SKILL_EN
        .chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
}
/// `--name` words in a text, without `--runtime` (every command takes it) and the
/// command's own name (`--skill`).
fn flags_in(text: &str, own: &str) -> std::collections::BTreeSet<String> {
    text.split("--")
        .skip(1)
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                .collect::<String>()
        })
        .filter(|f| !f.is_empty() && f != "runtime" && format!("--{f}") != own)
        .collect()
}
fn declared(c: &Command) -> std::collections::BTreeSet<String> {
    c.valued
        .iter()
        .chain(c.switches)
        .map(|f| f.to_string())
        .collect()
}
#[test]
fn every_command_documents_exactly_the_flags_it_takes() {
    for c in COMMANDS {
        let en = mooring_core::i18n::english(c.usage)
            .unwrap_or_else(|| panic!("missing English usage for {}", c.name));
        for (lang, text) in [("zh", c.usage), ("en", en)] {
            assert_eq!(flags_in(text, c.name), declared(c), "{lang}: {}", c.name);
            for line in text.lines().filter(|l| !l.starts_with(' ')) {
                assert!(
                    line.starts_with(&format!("fibo {}", c.name)),
                    "{lang}: {line}"
                );
            }
        }
        assert!(usage().contains(c.usage()), "{}", c.name);
    }
    // `fibo plugins` prints its own fuller text on errors; it names the same flags in both languages.
    let plugins = command("plugins").unwrap();
    for (lang, text) in [
        ("zh", crate::plugins_cmd::USAGE),
        (
            "en",
            mooring_core::i18n::english(crate::plugins_cmd::USAGE).unwrap(),
        ),
    ] {
        let mut own = flags_in(text, "--skill");
        own.remove("help");
        assert_eq!(own, declared(plugins), "{lang}");
    }
    for name in DRIVES {
        assert!(
            command(name).is_some(),
            "{name} is dispatched but has no usage"
        );
    }
}
#[test]
fn every_documented_flag_is_accepted_and_nothing_else() {
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for c in COMMANDS {
        for f in c.valued {
            let given = args(&["x", &format!("--{f}"), "1"]);
            assert!(!check(c, &given).unwrap(), "{} --{f}", c.name);
            let e = check(c, &args(&["x", &format!("--{f}")])).unwrap_err();
            assert!(
                e.to_string().contains("需要一个值") || e.to_string().contains("requires a value"),
                "{e}"
            );
        }
        for f in c.switches {
            let given = args(&["x", &format!("--{f}"), "--runtime", "/r"]);
            assert!(!check(c, &given).unwrap(), "{} --{f}", c.name);
        }
        let e = check(c, &args(&["x", "--timout", "60"])).unwrap_err();
        assert!(e.to_string().contains("--timout"), "{e}");
        assert!(e.to_string().contains(c.usage()), "{e}");
        // Help anywhere wins over everything, so the command never acts.
        assert!(check(c, &args(&["x", "--help"])).unwrap(), "{}", c.name);
        assert!(check(c, &args(&["-h", "--timout"])).unwrap(), "{}", c.name);
    }
    let split = command("split").unwrap();
    // After a bare `--` the words are the command's, its `--help` included.
    assert!(!check(split, &args(&["x", "--", "ls", "--help", "--bogus"])).unwrap());
    assert!(Args::parse(
        args(&["x", "--wiat"]).into_iter(),
        command("prompt").unwrap()
    )
    .is_err());
    let a = Args::parse(
        args(&["x", "--wait", "--timeout", "5"]).into_iter(),
        command("prompt").unwrap(),
    )
    .unwrap();
    assert!(a.has("wait") && a.value("timeout") == Some("5"));
}
#[test]
fn standard_input_past_the_limit_is_an_error_not_a_cut() {
    assert_eq!(read_limited(&b"abcd"[..], 4).unwrap(), "abcd");
    let e = read_limited(&b"abcde"[..], 4).unwrap_err();
    assert!(
        e.to_string().contains("超过") || e.to_string().contains("exceeds"),
        "{e}"
    );
    assert!(read_limited(&[0xff, 0xfe][..], 4).is_err());
}
#[test]
fn a_split_runs_its_command_in_a_shell_that_stays_open() {
    assert_eq!(split_argv("/bin/zsh", &[]), ["/bin/zsh", "-l"]);
    let one = split_argv("/bin/zsh", &["npm test && echo ok".into()]);
    assert_eq!(one[4], "npm test && echo ok; exec '/bin/zsh' -l");
    let words = split_argv("/bin/zsh", &["echo".into(), "a b".into()]);
    assert_eq!(&words[..4], ["/bin/zsh", "-l", "-i", "-c"]);
    assert_eq!(words[4], "'echo' 'a b'; exec '/bin/zsh' -l");
}
#[test]
fn everything_after_a_bare_double_dash_is_the_command() {
    let a = Args::parse(
        ["t", "--down", "--size", "0.3", "--", "ls", "--all"]
            .into_iter()
            .map(String::from),
        command("split").unwrap(),
    )
    .unwrap();
    assert_eq!(a.positional, ["t"]);
    assert!(a.has("down"));
    assert_eq!(a.value("size"), Some("0.3"));
    assert_eq!(a.rest, ["ls", "--all"]);
    assert!(!a.has("all"));
}
/// `mooring list --json` says what the window says, on every sample case.
#[test]
fn list_reads_every_field_from_its_owner() {
    let w = mooring_core::sample::every_state();
    let at = mooring_core::content::now();
    let rows = list_json(&w);
    assert!(!rows.is_empty());
    for row in rows {
        let t = row["id"].as_str().unwrap();
        let Some(s) = w.current_agent(t) else {
            continue;
        };
        let st = status::of(&w, s, at);
        assert_eq!(row["status"], st.key(), "{t}");
        assert_eq!(row["status_label"], st.label(), "{t}");
        assert_eq!(row["phase"], st.phase().key(), "{t}");
        assert_eq!(row["title"], w.session_title(s), "{t}");
        assert_eq!(row["tab"], w.terminal_tab_title(t), "{t}");
        let term = w.terminals.iter().find(|x| x.id == t).unwrap();
        assert_eq!(row["cwd"], w.terminal_cwd(term), "{t}");
    }
}
#[test]
fn phase_follows_the_current_conversation() {
    let mut w = fixture();
    assert_eq!(phase(&w, "aaaa2222"), Phase::None);
    assert_eq!(phase(&w, "aaaa1111"), Phase::Idle);
    use mooring_core::content::Turn;
    let mut c = mooring_core::content::Content {
        turn: Turn::Waiting,
        ..Default::default()
    };
    w.content.insert("s1".into(), c.clone());
    assert_eq!(phase(&w, "aaaa1111"), Phase::Waiting);
    c.turn = Turn::Running;
    w.content.insert("s1".into(), c.clone());
    assert_eq!(phase(&w, "aaaa1111"), Phase::Working);
    // A failed turn reads as the window reads it, not as a plain idle.
    c.turn = Turn::Error;
    w.content.insert("s1".into(), c);
    assert_eq!(phase(&w, "aaaa1111"), Phase::Idle);
    assert_eq!(status(&w, "aaaa1111"), Some(Status::Error));
}
