//! A terminal tied to the conversation its Agent was started with. The screens are real
//! captures from the RC3 desktop walk (2026-11-17, `tests/fixtures/started/`): a phone's
//! 新任务 handed to OpenCode 1.19.5 (`┃` before every line of the task, wrapped ones too)
//! and to Codex 0.169 (`code` drawn without its backticks). Neither shows the task verbatim,
//! and a task on the command line has no Enter press, so before the host kept what it
//! started them with, neither ever bound.
use super::launched::{evidence, launched, resume, same_message, words, First};
use super::*;
use fibo_core::model::{AgentStart, Space, Terminal};

const OPENCODE_TWO_LINES: &str =
    include_str!("../../tests/fixtures/started/opencode-two-lines.txt");
const OPENCODE_WRAPPED: &str = include_str!("../../tests/fixtures/started/opencode-wrapped.txt");
const OPENCODE_QUOTES_DASH: &str =
    include_str!("../../tests/fixtures/started/opencode-quotes-dash.txt");
const CODEX_BACKTICKS: &str = include_str!("../../tests/fixtures/started/codex-backticks.txt");
/// Codex 0.169 after a two-line task from the phone, drawn verbatim (RC4 check, 2026-11-17).
const CODEX_SAME_TASK: &str = include_str!("../../tests/fixtures/started/codex-same-task.txt");
const SAME_TASK: &str =
    "Two panes, one task, same folder.\nreply with only the uppercase form of rc4-codex-same-ok";

/// What each pane was started with, exactly as the phone sent it in that walk.
fn walked() -> Vec<(Agent, &'static str, &'static str)> {
    vec![
        (
            opencode(),
            OPENCODE_TWO_LINES,
            "Line one of two.\nReply with only WALK-RC3-NL-OK",
        ),
        (
            opencode(),
            OPENCODE_WRAPPED,
            "This single line is deliberately long so that it wraps inside the OpenCode prompt box at one hundred columns; reply with only WALK-RC3-WRAP-OK",
        ),
        (
            opencode(),
            OPENCODE_QUOTES_DASH,
            "--rc3-walk: keep \"double quotes\", 'single quotes' and $(echo not-expanded) exactly as written.\nLine two: reply with only WALK-RC3-REMOTE-OK",
        ),
        (
            Agent::Codex,
            CODEX_BACKTICKS,
            "This directory has two scripts. Start `sh serve.sh` and `sh job.sh` as two separate background terminals (they must keep running after your turn), do not wait for them or poll them, and end your turn right away. Reply only: started.",
        ),
    ]
}

fn opencode() -> Agent {
    Agent::from_key("opencode")
}

fn log(runtime: Agent, i: usize) -> Transcript {
    Transcript {
        runtime,
        cli_id: format!("t{i}"),
        path: format!("/logs/t{i}.jsonl"),
        cwd: "/w".into(),
        updated: 1000,
    }
}

/// A thread whose log holds `prompt` as its first and newest user message, at `at`.
fn thread<'a>(t: &'a Transcript, prompt: &str, at: u64) -> ScreenThread<'a> {
    ScreenThread {
        transcript: t,
        key: screen_key(prompt),
        at: Some(at),
        began: None,
        cwd: t.cwd.clone(),
        first: Some(First {
            text: screen_text(prompt),
            at,
            newest: true,
        }),
    }
}

/// A thread that began with `prompt` at `began`, and whose newest prompt is `later`, at `at`.
fn went_on<'a>(
    t: &'a Transcript,
    prompt: &str,
    began: u64,
    later: &str,
    at: u64,
) -> ScreenThread<'a> {
    ScreenThread {
        key: screen_key(later),
        at: Some(at),
        first: Some(First {
            text: screen_text(prompt),
            at: began,
            newest: false,
        }),
        ..thread(t, prompt, began)
    }
}

/// An unbound pane of `runtime` showing `screen`, started (or not) with `prompt` at 990.
fn pane(i: usize, runtime: Agent, screen: &str, prompt: Option<&str>) -> ScreenTerminal {
    let start = AgentStart {
        at: 990,
        ..AgentStart::new(runtime, prompt.map(str::to_owned))
    };
    ScreenTerminal {
        terminal: format!("pty-{i}"),
        runtime,
        cwd: Some("/w".into()),
        screen: screen_text(screen),
        current: None,
        submits: vec![],
        launched: launched(&start, runtime, "/w".into()),
    }
}

fn bound(threads: &[ScreenThread], panes: &[ScreenTerminal]) -> Vec<(String, String)> {
    screen_bindings(threads, panes)
        .into_iter()
        .map(|(pane, t)| (pane, t.cli_id.clone()))
        .collect()
}

#[test]
fn a_task_the_screen_redraws_binds_by_what_the_host_started_it_with() {
    for (i, (runtime, screen, prompt)) in walked().into_iter().enumerate() {
        let t = log(runtime, i);
        let threads = [thread(&t, prompt, 1000)];
        // The old evidence: the task verbatim on the screen, or Enter. Neither is there.
        assert!(
            !prompt_evidence(&threads[0], &screen_text(screen), &[]),
            "{runtime:?} shows the task verbatim after all: {screen}"
        );
        // A pane nothing says it started with this task (a host from before kept none):
        // unbound, as it has always been.
        assert!(bound(&threads, &[pane(0, runtime, screen, None)]).is_empty());
        assert_eq!(
            bound(&threads, &[pane(0, runtime, screen, Some(prompt))]),
            vec![("pty-0".to_owned(), format!("t{i}"))],
            "{runtime:?}: {prompt}"
        );
    }
}

/// Binding a pane to another pane's conversation is worse than binding none: phone chat
/// would type into the wrong Agent.
#[test]
fn two_panes_started_with_one_task_in_one_folder_bind_neither() {
    let prompt = "Line one of two.\nReply with only WALK-RC3-NL-OK";
    let (a, b) = (log(opencode(), 0), log(opencode(), 1));
    let both = [thread(&a, prompt, 1000), thread(&b, prompt, 1001)];
    let panes = [
        pane(0, opencode(), OPENCODE_TWO_LINES, Some(prompt)),
        pane(1, opencode(), OPENCODE_TWO_LINES, Some(prompt)),
    ];
    assert!(bound(&both, &panes).is_empty());
    // Only one of them has logged yet: it fits both panes, which binds neither.
    assert!(bound(&both[..1], &panes).is_empty());
    // One pane, two conversations begun with its task: neither is surely its own.
    assert!(bound(&both, &panes[..1]).is_empty());
}

/// What tells two such panes apart is the next prompt typed into either. Enter in pane 0 for
/// a later prompt of its thread: that thread goes by the Enter, not by what it began with;
/// the pane no longer by its start; and pane 1 is left with the other thread.
#[test]
fn the_next_prompt_typed_into_one_of_two_panes_started_alike_tells_them_apart() {
    let prompt = "Line one of two.\nReply with only WALK-RC3-NL-OK";
    let typed = "现在只看这一块：把第二行改成英文再回答";
    let (a, b) = (log(opencode(), 0), log(opencode(), 1));
    let threads = [
        went_on(&a, prompt, 1000, typed, 1010),
        thread(&b, prompt, 1001),
    ];
    let mut panes = [
        pane(0, opencode(), OPENCODE_TWO_LINES, Some(prompt)),
        pane(1, opencode(), OPENCODE_TWO_LINES, Some(prompt)),
    ];
    // A later prompt, but no Enter for it in either pane: nothing says where it went.
    assert!(bound(&threads, &panes).is_empty());
    panes[0].submits = vec![1010];
    assert_eq!(
        bound(&threads, &panes),
        [("pty-0", "t0"), ("pty-1", "t1")].map(|(p, t)| (p.to_owned(), t.to_owned()))
    );
    // Pane 0 runs its thread now (and its start is used up): pane 1 still gets only its own.
    panes[0].current = Some(("t0".into(), 1010, screen_key(typed)));
    panes[0].launched = None;
    assert_eq!(
        bound(&threads, &panes),
        vec![("pty-1".to_owned(), "t1".to_owned())]
    );
}

/// A pane started with a task that never bound, then typed into (`/new`, another session
/// picked): Enter sent another thread's first prompt there, and the pane goes by that.
#[test]
fn a_pane_typed_into_after_its_start_goes_by_what_was_typed() {
    let prompt = "Line one of two.\nReply with only WALK-RC3-NL-OK";
    let typed = "新开一段：先列出这个目录里的脚本";
    let (task, fresh) = (log(opencode(), 0), log(opencode(), 1));
    let mut p = pane(0, opencode(), OPENCODE_TWO_LINES, Some(prompt));
    p.submits = vec![1010];
    let threads = [thread(&task, prompt, 1000), thread(&fresh, typed, 1010)];
    assert_eq!(
        bound(&threads, &[p]),
        vec![("pty-0".to_owned(), "t1".to_owned())]
    );
}

/// Enter for the task itself (a trust question answered just as the task is logged) is no
/// newer word than the start: one pane still binds, and two started alike still bind neither,
/// each Enter near its own thread's log time or not.
#[test]
fn enter_for_the_task_itself_is_no_newer_word() {
    let prompt = "Line one of two.\nReply with only WALK-RC3-NL-OK";
    let (a, b) = (log(opencode(), 0), log(opencode(), 1));
    let mut one = pane(0, opencode(), OPENCODE_TWO_LINES, Some(prompt));
    one.submits = vec![999];
    assert_eq!(
        bound(&[thread(&a, prompt, 1000)], &[one]),
        vec![("pty-0".to_owned(), "t0".to_owned())]
    );
    let mut panes = [
        pane(0, opencode(), OPENCODE_TWO_LINES, Some(prompt)),
        pane(1, opencode(), OPENCODE_TWO_LINES, Some(prompt)),
    ];
    (panes[0].submits, panes[1].submits) = (vec![999], vec![1010]);
    let threads = [thread(&a, prompt, 1000), thread(&b, prompt, 1010)];
    assert!(bound(&threads, &panes).is_empty());
}

/// Codex draws a two-line task verbatim (real capture), so each of two panes started with it
/// shows the other pane's thread too. That drawing is the start's own evidence, not the
/// screen's: once Enter sends pane 0's thread on, neither pane shows the other's any more and
/// both bind, each to its own (RC4 check: before this, typing into one bound neither).
#[test]
fn a_pane_drawing_its_own_task_is_the_starts_evidence_not_the_screens() {
    let typed = "Only pane a goes on: reply with only the uppercase form of rc4-codex-same2-a";
    let (a, b) = (log(Agent::Codex, 0), log(Agent::Codex, 1));
    let threads = [
        went_on(&a, SAME_TASK, 1000, typed, 1040),
        thread(&b, SAME_TASK, 1007),
    ];
    assert!(
        prompt_evidence(&threads[1], &screen_text(CODEX_SAME_TASK), &[]),
        "Codex shows the task verbatim after all"
    );
    let mut panes = [
        pane(0, Agent::Codex, CODEX_SAME_TASK, Some(SAME_TASK)),
        pane(1, Agent::Codex, CODEX_SAME_TASK, Some(SAME_TASK)),
    ];
    assert!(bound(&threads, &panes).is_empty(), "nothing typed yet");
    panes[0].submits = vec![1040];
    assert_eq!(
        bound(&threads, &panes),
        [("pty-0", "t0"), ("pty-1", "t1")].map(|(p, t)| (p.to_owned(), t.to_owned()))
    );
    // An older conversation that began with the same words, before this pane was started:
    // the pane drawing its own task says nothing about that one.
    let old = log(Agent::Codex, 2);
    let started = || pane(0, Agent::Codex, CODEX_SAME_TASK, Some(SAME_TASK));
    assert!(bound(&[thread(&old, SAME_TASK, 980)], &[started()]).is_empty());
    // A pane nothing says was started with it (a host from before kept none): the screen
    // binds as it always has.
    let mut plain = started();
    plain.launched = None;
    assert_eq!(
        bound(&[thread(&old, SAME_TASK, 980)], &[plain]),
        vec![("pty-0".to_owned(), "t2".to_owned())]
    );
}

#[test]
fn only_a_conversation_begun_with_that_task_after_the_start_in_its_folder_and_cli() {
    let prompt = "Line one of two.\nReply with only WALK-RC3-NL-OK";
    let t = log(opencode(), 0);
    let ok = || pane(0, opencode(), OPENCODE_TWO_LINES, Some(prompt));
    assert_eq!(bound(&[thread(&t, prompt, 990)], &[ok()]).len(), 1);
    // Begun before the host started this pane: an older conversation with the same words.
    assert!(bound(&[thread(&t, prompt, 989)], &[ok()]).is_empty());
    // Begun with something else, or with the task as a later message only.
    let other = thread(&t, "Line one of two.\nReply with only WALK-RC3-NL-NO", 1000);
    assert!(bound(&[other], &[ok()]).is_empty());
    let later = ScreenThread {
        first: Some(First {
            text: screen_text("先看一下目录结构"),
            at: 995,
            newest: false,
        }),
        ..thread(&t, prompt, 1000)
    };
    assert!(bound(&[later], &[ok()]).is_empty());
    // Another folder.
    let mut elsewhere = ok();
    elsewhere.cwd = Some("/other".into());
    assert!(bound(&[thread(&t, prompt, 1000)], &[elsewhere]).is_empty());
    // A pane whose process cwd could not be read is compared by where it was started.
    let mut unread = ok();
    unread.cwd = None;
    assert_eq!(bound(&[thread(&t, prompt, 1000)], &[unread]).len(), 1);
    // Another CLI: the start says which Agent, and a pane runs only its own threads.
    let codex = log(Agent::Codex, 1);
    assert!(bound(&[thread(&codex, prompt, 1000)], &[ok()]).is_empty());
    let start = AgentStart {
        at: 990,
        ..AgentStart::new(Agent::Codex, Some(prompt.into()))
    };
    assert_eq!(launched(&start, opencode(), "/w".into()), None);
    // A pane that runs a conversation already: what it was started with is used up.
    let mut busy = ok();
    busy.current = Some(("other".into(), 999, None));
    assert!(!evidence(&thread(&t, prompt, 1000), &busy));
}

#[test]
fn the_whole_message_counts_unless_the_log_keeps_only_its_start() {
    assert!(same_message("abc", "abc"));
    assert!(!same_message("abc", "abcd"), "a prefix is another message");
    assert!(!same_message("", ""), "nothing logged says nothing");
    // OpenCode's records keep 4000 characters of a message; a todo's handoff can be longer.
    let long: String = "第一步先读一下现有的测试再动手".repeat(300);
    let kept: String = long.chars().take(4000).collect();
    assert!(same_message(&words(&kept), &words(&long)));
    let short: String = long.chars().take(400).collect();
    assert!(!same_message(&words(&short), &words(&long)));
    // Both sides drop what a log drops from the user's words, and all whitespace.
    let sent = "修这个错误\n```\npanic at main.rs:3\n```\n> 引用的旧回复\n不要改接口";
    assert_eq!(
        words(sent),
        screen_text(&crate::content::user_text(sent)),
        "the log's own reading of the message"
    );
    assert_eq!(words(sent), "修这个错误不要改接口");
}

fn terminal(id: &str, pid: u32) -> Terminal {
    Terminal {
        id: id.into(),
        space: "s".into(),
        argv: vec!["sh".into()],
        cwd: "/w".into(),
        pid: Some(pid),
        state: ProcessState::Running,
        exit_code: None,
        error: None,
    }
}

fn process(pid: u32, parent: u32, runtime: Option<Agent>, id: Option<&str>) -> Process {
    Process {
        pid,
        parent,
        runtime,
        cli_session_id: id.map(str::to_owned),
        no_daemon: false,
        launch: vec![],
    }
}

/// `opencode --session ses_x` names no conversation Fibo reads, and resuming one writes
/// nothing to its log until it is spoken to: the phone's chat had no conversation to send
/// to, so the first message it needed could never be sent (RC3 walk).
#[test]
fn a_terminal_started_resuming_binds_that_conversation_once_its_cli_runs_there() {
    let w = || {
        let mut w = Workspace {
            spaces: vec![Space {
                id: "s".into(),
                name: "w".into(),
                cwd: "/w".into(),
            }],
            ..Default::default()
        };
        w.terminals.push(terminal("resumed", 100));
        w.agent_starts.insert(
            "resumed".into(),
            AgentStart {
                at: 990,
                ..AgentStart::resuming(opencode(), "ses_x")
            },
        );
        w
    };
    let logs = [Transcript {
        cli_id: "ses_x".into(),
        ..log(opencode(), 0)
    }];
    let shell = process(100, 1, None, None);
    let cli = process(101, 100, Some(opencode()), None);
    // Its shell alone, the CLI not started yet: nothing to tie.
    let mut early = w();
    resume(&mut early, &logs, std::slice::from_ref(&shell));
    assert!(early.current_agent("resumed").is_none());
    let mut ready = w();
    resume(&mut ready, &logs, &[shell.clone(), cli.clone()]);
    let s = ready
        .current_agent("resumed")
        .expect("tied once its CLI runs");
    assert_eq!(
        (s.cli_session_id.as_str(), s.binding),
        ("ses_x", Some(Binding::Screen))
    );
    // Its log unknown: nothing to tie it to.
    let mut unknown = w();
    resume(&mut unknown, &[], &[shell.clone(), cli.clone()]);
    assert!(unknown.current_agent("resumed").is_none());
    // A command line that names another conversation is that one's (`reconcile`).
    let mut named = w();
    let other = process(101, 100, Some(opencode()), Some("ses_y"));
    resume(&mut named, &logs, &[shell.clone(), other]);
    assert!(named.current_agent("resumed").is_none());
    // Another CLI in the terminal is not the one started there.
    let mut wrong = w();
    let codex = process(101, 100, Some(Agent::Codex), None);
    resume(&mut wrong, &logs, &[shell.clone(), codex]);
    assert!(wrong.current_agent("resumed").is_none());
    // Two of it in one terminal: which one resumed it, nothing says.
    let mut two = w();
    let second = process(102, 100, Some(opencode()), None);
    resume(&mut two, &logs, &[shell.clone(), cli.clone(), second]);
    assert!(two.current_agent("resumed").is_none());
    // Still running in another terminal: it stays there.
    let mut elsewhere = w();
    elsewhere.terminals.push(terminal("old", 200));
    elsewhere
        .observe_agent(super::bound(&logs[0], "old".into(), Binding::Screen))
        .unwrap();
    resume(&mut elsewhere, &logs, &[shell, cli]);
    assert_eq!(
        elsewhere
            .current_agent("old")
            .map(|s| s.cli_session_id.as_str()),
        Some("ses_x")
    );
    assert!(elsewhere.current_agent("resumed").is_none());
}
