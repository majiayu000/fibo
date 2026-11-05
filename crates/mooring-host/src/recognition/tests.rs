use super::processes::{agent_named, env_has, env_value, hinted_roots};
use super::*;
use crate::agents::{claude::claude_session_id, codex::codex_args};
#[test]
fn a_hook_transcript_belongs_to_the_cli_whose_root_holds_it() {
    use super::{transcript_owner, Agent, Path};
    let roots = crate::agents::roots(Path::new("/nohome"));
    let grok = Path::new("/nohome/.grok/sessions/%2Fx/01a1/updates.jsonl");
    let claude = Path::new("/nohome/.claude/projects/-x/abc.jsonl");
    // Grok runs the hook from Claude's settings with `--agent claude`.
    assert_eq!(transcript_owner(grok, &roots), Some(Agent::Grok));
    assert_eq!(transcript_owner(claude, &roots), Some(Agent::Claude));
    assert_eq!(
        transcript_owner(Path::new("/elsewhere/a.jsonl"), &roots),
        None
    );
}
fn procargs_of(argv: &[&str], env: &[&str]) -> Vec<u8> {
    let mut b = (argv.len() as i32).to_ne_bytes().to_vec();
    b.extend(b"/usr/bin/nono\0\0\0");
    for a in argv.iter().chain(env) {
        b.extend(a.as_bytes());
        b.push(0);
    }
    b.extend(b"\0executable_path=/usr/bin/nono\0");
    b
}
#[test]
fn agent_hint_reads_only_its_own_variable_after_argv() {
    let b = procargs_of(
        &["nono", "MOORING_AGENT=codex"],
        &["HOME=/x", "MOORING_AGENT=Claude"],
    );
    assert_eq!(
        env_value(&b, "MOORING_AGENT").as_deref(),
        Some("Claude"),
        "argv is not the environment"
    );
    assert_eq!(
        env_value(&procargs_of(&["nono"], &["HOME=/x"]), "MOORING_AGENT"),
        None
    );
    assert_eq!(
        env_value(
            &procargs_of(&["nono"], &["MOORING_AGENTX=claude"]),
            "MOORING_AGENT"
        ),
        None
    );
    assert_eq!(agent_named("Claude"), Some(Agent::Claude));
    assert_eq!(agent_named(" codex "), Some(Agent::Codex));
    assert_eq!(agent_named("vim"), None);
}
#[test]
fn agent_hint_counts_the_outermost_wrapper_unless_the_cli_shows_below() {
    let c = Some(Agent::Claude);
    // shell 1 → wrapper 2 (hint) → child 3 (inherits hint) → tool 4 (inherits hint).
    let wrapped = [
        (1, 0, None, None),
        (2, 1, None, c),
        (3, 2, None, c),
        (4, 3, None, c),
    ];
    assert_eq!(hinted_roots(&wrapped), HashMap::from([(2, Agent::Claude)]));
    // The real CLI is visible under the wrapper: it is counted by name, the hint is not.
    let visible = [(1, 0, None, None), (2, 1, None, c), (3, 2, c, None)];
    assert!(hinted_roots(&visible).is_empty());
}

#[test]
fn screen_wait_reads_only_strict_approval_and_choice_shapes() {
    let claude_approval = "⏺ Bash(git push origin feat/feedback)\n\n╭────────────────────────────╮\n│ Bash command               │\n│                            │\n│   git push origin feat/x   │\n│                            │\n│ Do you want to proceed?    │\n│ ❯ 1. Yes                   │\n│   2. Yes, and don't ask again for git push commands │\n│   3. No, and tell Claude what to do differently (esc) │\n╰────────────────────────────╯\n";
    assert_eq!(
        screen_wait(claude_approval).as_deref(),
        Some("Do you want to proceed?")
    );
    let codex_approval = "• Running cargo test\n\n  Would you like to run the following command?\n\n  $ cargo test --workspace\n\n› 1. Yes, proceed (y)\n  2. Yes, and don't ask again for this command in this session (a)\n  3. No, and tell Codex what to do differently (esc)\n\n  Press enter to confirm or esc to cancel\n";
    assert_eq!(
        screen_wait(codex_approval).as_deref(),
        Some("Would you like to run the following command?")
    );
    // Transcribed from the R9 real Claude Code 2.1.302 screen (evidence/r9-20261021/real-approval.png):
    // the approval R9 could not see from the log alone.
    let r9_claude_write = "● Write(approval-proof.txt)\n\n────────────────────────────────────────\n Create file\n approval-proof.txt\n╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌\n  1 ok\n╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌\n Do you want to create approval-proof.txt?\n ❯ 1. Yes\n   2. Yes, and switch to accept edits (auto-approve file edits and common\n      file commands) for this session (shift+tab)\n   3. No\n\n Esc to cancel · Tab to amend\n";
    assert_eq!(
        screen_wait(r9_claude_write).as_deref(),
        Some("Do you want to create approval-proof.txt?")
    );
    let choice = "☐ 部署\n\n要部署到哪个环境？\n\n❯ 1. staging\n     先在预发验证\n  2. production\n  3. Type something.\n\nEnter to select · ↑/↓ to navigate · Esc to cancel\n";
    assert_eq!(screen_wait(choice).as_deref(), Some("要部署到哪个环境？"));

    // A transcript that merely mentions the words, a plain numbered list in the
    // agent's answer, a menu without a cursor, and a stale footer far above all stay
    // unclassified.
    for text in [
        "Do you want to proceed? I think we should.\n1. Refactor\n2. Test\n",
        "The plan:\n1. Yes we can split it\n2. Then test\n\n> ",
        "Do you want to proceed?\n\n(answered)\n⏺ Running tests…\n",
        "Do you want to proceed?\n❯ 1. Yes\n  2. No\n⏺ Bash(ls)\n  ⎿ a\n  ⎿ b\n  ⎿ c\n  ⎿ d\n",
        "Pick one\n1. a\n2. b\nEnter to select · Esc to cancel\n",
        "Pick one\n❯ 1. a\n2. b\nEnter to select · Esc to cancel\nline\nline\nline\nline\n",
        "$ ls\nsrc  tests\n$ ",
    ] {
        assert_eq!(screen_wait(text), None, "{text}");
    }
}
#[test]
fn transcript_off_reads_only_the_environment_tail() {
    let procargs = |argv: &[&str], env: &[&str]| {
        let mut b = (argv.len() as i32).to_ne_bytes().to_vec();
        b.extend_from_slice(b"/usr/local/bin/claude\0\0\0");
        for part in argv.iter().chain(env) {
            b.extend_from_slice(part.as_bytes());
            b.push(0);
        }
        b
    };
    let key = "CLAUDE_CODE_CHILD_SESSION";
    assert!(env_has(
        &procargs(&["claude"], &["HOME=/u", "CLAUDE_CODE_CHILD_SESSION=1"]),
        key
    ));
    // A prompt that names the variable, a longer name, or no marker at all.
    for (argv, env) in [
        (
            &["claude", "CLAUDE_CODE_CHILD_SESSION=1"][..],
            &["HOME=/u"][..],
        ),
        (&["claude"], &["CLAUDE_CODE_CHILD_SESSION_X=1"]),
        (&["claude"], &[]),
    ] {
        assert!(!env_has(&procargs(argv, env), key), "{argv:?} {env:?}");
    }
    assert!(!env_has(b"\0\0", key));
}
#[test]
fn explicit_claude_id_uses_argv_boundaries_and_kernel_arguments() {
    let id = uuid::Uuid::new_v4().to_string();
    for flag in ["--session-id", "--resume", "-r"] {
        let args = vec!["claude".into(), flag.into(), id.clone(), "say ok".into()];
        assert_eq!(claude_session_id(&args), Some(id.clone()));
    }
    assert_eq!(
        claude_session_id(&["claude".into(), format!("--session-id={id}")]),
        Some(id.clone())
    );
    for args in [
        vec!["claude".into(), format!("please explain --session-id {id}")],
        vec![
            "claude".into(),
            "--".into(),
            "--session-id".into(),
            id.clone(),
        ],
        vec!["claude".into(), "--session-id".into(), "invalid".into()],
        vec![
            "claude".into(),
            "--session-id".into(),
            id.clone(),
            "--resume".into(),
            id,
        ],
    ] {
        assert_eq!(claude_session_id(&args), None);
    }
    assert_eq!(
        process_argv(std::process::id()).unwrap(),
        std::env::args().collect::<Vec<_>>()
    );
}

#[test]
fn codex_resume_uuid_is_exact_and_everything_else_is_not_identity() {
    let id = uuid::Uuid::new_v4().to_string();
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for args in [
        v(&["codex", "resume", &id]),
        v(&["codex", "--no-daemon", "resume", &id, "继续"]),
        v(&["codex", "-m", "gpt", "resume", "-c", "a=1", &id]),
        v(&[
            "codex",
            "--dangerously-bypass-approvals-and-sandbox",
            "resume",
            "--",
            &id,
        ]),
    ] {
        assert_eq!(codex_args(&args).0, Some(id.clone()), "{args:?}");
    }
    for args in [
        v(&["codex"]),
        v(&["codex", &format!("resume {id}")]),
        v(&["codex", "fix", "resume", &id]),
        v(&["codex", "fork", &id]),
        v(&["codex", "resume", "--last"]),
        v(&["codex", "resume", "my thread name"]),
        v(&["codex", "-m", "resume", &id]),
    ] {
        assert_eq!(codex_args(&args).0, None, "{args:?}");
    }
    assert!(codex_args(&v(&["codex", "--no-daemon"])).1);
    assert!(!codex_args(&v(&["codex", "--", "--no-daemon"])).1);
}

/// (current thread id, its last update, its newest prompt)
type Current<'a> = Option<(&'a str, u64, &'a str)>;
fn screen_case(prompts: &[(&str, u64)], screens: &[(&str, Current)]) -> Vec<(String, String)> {
    let transcripts: Vec<_> = prompts
        .iter()
        .enumerate()
        .map(|(i, (_, updated))| Transcript {
            runtime: Agent::Codex,
            cli_id: format!("t{i}"),
            path: format!("/logs/t{i}.jsonl"),
            cwd: "/same".into(),
            updated: *updated,
        })
        .collect();
    let threads: Vec<_> = transcripts
        .iter()
        .zip(prompts)
        .filter_map(|(transcript, (prompt, _))| {
            Some(ScreenThread {
                transcript,
                key: Some(screen_key(prompt)?),
                at: None,
                began: None,
                cwd: "/same".into(),
            })
        })
        .collect();
    let terminals: Vec<_> = screens
        .iter()
        .enumerate()
        .map(|(i, (screen, current))| ScreenTerminal {
            terminal: format!("pty-{i}"),
            runtime: Agent::Codex,
            cwd: Some("/same".into()),
            screen: screen_text(screen),
            current: current.map(|(id, at, prompt)| (id.into(), at, screen_key(prompt))),
            submits: vec![],
        })
        .collect();
    screen_bindings(&threads, &terminals)
        .into_iter()
        .map(|(terminal, t)| (terminal, t.cli_id.clone()))
        .collect()
}

#[test]
fn a_thread_binds_only_to_a_screen_of_its_own_cli() {
    let prompt = "把左侧会话列表按最近活动排序";
    let transcript = |runtime| Transcript {
        runtime,
        cli_id: "c".into(),
        path: "/logs/c.jsonl".into(),
        cwd: "/same".into(),
        updated: 1,
    };
    let pane = |runtime| ScreenTerminal {
        terminal: "pty".into(),
        runtime,
        cwd: Some("/same".into()),
        screen: screen_text(prompt),
        current: None,
        submits: vec![],
    };
    for (thread, screen, binds) in [
        (Agent::Claude, Agent::Claude, true),
        (Agent::Grok, Agent::Grok, true),
        (Agent::Claude, Agent::Codex, false),
        (Agent::Codex, Agent::Claude, false),
    ] {
        let t = transcript(thread);
        let threads = [ScreenThread {
            transcript: &t,
            key: screen_key(prompt),
            at: None,
            began: None,
            cwd: "/same".into(),
        }];
        assert_eq!(
            screen_bindings(&threads, &[pane(screen)]).len(),
            usize::from(binds),
            "{thread:?} thread on a {screen:?} screen"
        );
    }
}

#[test]
fn screen_prompt_binds_one_to_one_and_refuses_everything_ambiguous() {
    let a = "请检查登录页的跳转逻辑并修复回归";
    let b = "给设置页补上深色模式的截图测试";
    // Same cwd, wrapped differently on screen: each thread finds only its own pane.
    assert_eq!(
        screen_case(
            &[(a, 1), (b, 1)],
            &[
                ("› 请检查登录页的跳转逻辑\n  并修复回归\n", None),
                (format!("› {b}").as_str(), None)
            ]
        ),
        vec![("pty-0".into(), "t0".into()), ("pty-1".into(), "t1".into())]
    );
    // Identical prompts on two panes bind neither.
    assert!(screen_case(&[(a, 1), (a, 1)], &[(a, None), (a, None)]).is_empty());
    // One thread visible on two panes, or a pane showing two threads: nothing.
    assert!(screen_case(&[(a, 1)], &[(a, None), (a, None)]).is_empty());
    assert!(screen_case(&[(a, 1), (b, 1)], &[(format!("{a}\n{b}").as_str(), None)]).is_empty());
    // Short prompts are never evidence.
    assert!(screen_case(&[("继续", 1)], &[("继续", None)]).is_empty());
    // /new: a newer thread takes its pane; an older one never does.
    assert_eq!(
        screen_case(
            &[(b, 5)],
            &[(format!("{a}\n{b}").as_str(), Some(("old", 4, a)))]
        ),
        vec![("pty-0".into(), "t0".into())]
    );
    assert!(screen_case(&[(b, 3)], &[(b, Some(("old", 4, a)))]).is_empty());
    // The pane's current thread already has this prompt: the screen cannot tell them apart.
    assert!(screen_case(&[(a, 5)], &[(a, Some(("old", 4, a)))]).is_empty());
}

#[test]
fn enter_at_the_logged_moment_binds_short_and_pasted_prompts() {
    let t = |i: usize| Transcript {
        runtime: Agent::Codex,
        cli_id: format!("t{i}"),
        path: format!("/logs/t{i}.jsonl"),
        cwd: "/same".into(),
        updated: 100,
    };
    let (t0, t1) = (t(0), t(1));
    let thread = |transcript, at| ScreenThread {
        transcript,
        key: screen_key("继续"),
        at: Some(at),
        began: None,
        cwd: "/same".into(),
    };
    let pane = |i: usize, submits: Vec<u64>| ScreenTerminal {
        terminal: format!("pty-{i}"),
        runtime: Agent::Codex,
        cwd: Some("/same".into()),
        screen: screen_text("[Pasted Content 4096 chars]"),
        current: None,
        submits,
    };
    let bound = |threads: &[ScreenThread], panes: &[ScreenTerminal]| {
        screen_bindings(threads, panes)
            .into_iter()
            .map(|(pane, t)| (pane, t.cli_id.clone()))
            .collect::<Vec<_>>()
    };
    // Each pane pressed Enter when its own thread logged a prompt.
    assert_eq!(
        bound(
            &[thread(&t0, 1000), thread(&t1, 1060)],
            &[pane(0, vec![1001]), pane(1, vec![1059])]
        ),
        vec![("pty-0".into(), "t0".into()), ("pty-1".into(), "t1".into())]
    );
    // Enter too far from the logged time is no evidence.
    assert!(bound(&[thread(&t0, 1000)], &[pane(0, vec![1010])]).is_empty());
    // Shared-daemon Codex logs a thread's first prompt seconds after Enter, once the
    // thread is built; the turn it opened began at Enter.
    let first = ScreenThread {
        began: Some(1000),
        ..thread(&t0, 1006)
    };
    assert_eq!(
        bound(&[first], &[pane(0, vec![1000])]),
        vec![("pty-0".into(), "t0".into())]
    );
    let first = ScreenThread {
        began: Some(1000),
        ..thread(&t0, 1006)
    };
    assert!(bound(&[first], &[pane(0, vec![990])]).is_empty());
    // Two panes pressing Enter together in one directory: neither binds.
    assert!(bound(
        &[thread(&t0, 1000)],
        &[pane(0, vec![1000]), pane(1, vec![1001])]
    )
    .is_empty());
    // Another directory never binds.
    let mut elsewhere = pane(0, vec![1000]);
    elsewhere.cwd = Some("/other".into());
    assert!(bound(&[thread(&t0, 1000)], &[elsewhere]).is_empty());
}
