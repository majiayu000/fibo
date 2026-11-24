//! Text, named keys and raw input typed into a terminal from outside its pane.
use super::*;

/// A paste from outside the app, plus Enter as its own write: an Agent that sees the
/// Enter inside the paste would take it as a newline in the prompt, not a submit.
pub(in crate::host) fn send_text(
    shared: &Shared,
    id: &str,
    text: &str,
    submit: bool,
    session: Option<&str>,
) -> io::Result<()> {
    send_message(
        shared,
        id,
        text,
        submit.then_some(b"\r".as_slice()),
        session,
    )
}

pub(in crate::host) fn send_turn(
    shared: &Shared,
    id: &str,
    text: &str,
    steer: bool,
    session: Option<&str>,
) -> io::Result<()> {
    let keys = {
        let host = lock(shared)?;
        let agent = host
            .workspace
            .current_agent(id)
            .map(|s| s.runtime)
            .or_else(|| host.recognition.terminal_agents.get(id).copied());
        agent
            .and_then(|a| crate::agent_adapter(a).turn_keys())
            .ok_or_else(|| io::Error::other(t!("这个 Agent 分不出排队和马上发，直接发送就好")))?
    };
    if steer && text.is_empty() && !keys.send_now {
        return Err(io::Error::other(t!(
            "这个 Agent 要等手头这一步做完才会读排队的消息"
        )));
    }
    send_message(
        shared,
        id,
        text,
        Some(if steer { keys.steer } else { keys.queue }),
        session,
    )
}

fn send_message(
    shared: &Shared,
    id: &str,
    text: &str,
    submit: Option<&[u8]>,
    session: Option<&str>,
) -> io::Result<()> {
    if text.contains('\x1b') {
        return Err(io::Error::other(t!("文本包含终端控制字符")));
    }
    if text.len() > 64 * 1024 {
        return Err(io::Error::other(t!("单次输入超过 64 KiB")));
    }
    let host = lock(shared)?;
    check_session(&host.workspace, id, session)?;
    let terminal = host
        .terminals
        .get(id)
        .cloned()
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    let typing = {
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        message_allowed(&host, &live, id).map_err(|e| io::Error::other(NotTyped(e.to_string())))?;
        if !text.is_empty() {
            let bracketed = live
                .vt
                .terminal()
                .mode(fibo_core::terminal::libghostty_vt::terminal::Mode::BRACKETED_PASTE)
                .unwrap_or(false);
            let bytes = if bracketed {
                format!("\x1b[200~{text}\x1b[201~")
            } else if text.contains(['\n', '\r']) {
                return Err(io::Error::other(t!("程序未启用安全粘贴，不能发送多行文本")));
            } else {
                text.to_owned()
            };
            live.input_at = Some(Instant::now());
            Some((live.input.clone(), bytes))
        } else {
            None
        }
    };
    drop(host);
    // With no lock held: a program busy printing reads this only once its output is taken.
    if let Some((input, bytes)) = typing {
        input.write(bytes)?;
    }
    // Same as typing in the app: the prompt on screen is no longer waiting on the user.
    lock(shared)?.answer_screen_wait(id)?;
    let Some(submit) = submit else {
        return Ok(());
    };
    // Long enough for an Agent's paste handling to finish before Enter arrives.
    thread::sleep(Duration::from_millis(150));
    let input = {
        let host = lock(shared)?;
        check_session(&host.workspace, id, session)?;
        let live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        message_allowed(&host, &live, id)?;
        live.input.clone()
    };
    input.write(submit)?;
    typed(shared, id, submit.contains(&b'\r'))
}

/// A message refused before any of it was typed. Its request id is not used up
/// (`once`): the same message sent again after the prompt is answered gets typed.
#[derive(Debug)]
pub(in crate::host) struct NotTyped(String);
impl std::fmt::Display for NotTyped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for NotTyped {}

/// Inspect the current screen under the same locks used to write the message.
/// The second call catches an approval appearing while the paste is being handled.
fn message_allowed(host: &Host, live: &live::Live, id: &str) -> io::Result<()> {
    use crate::screen_lines::{open_menu_at_bottom, OpenMenu};
    let tail = live.vt.tail_text(usize::from(live.rows))?;
    // An open menu counts even when no rule read its question: a narrow pane once wrapped
    // Codex's question, and the message's Enter approved a command (11-17 walk).
    let menu = open_menu_at_bottom(&tail);
    if crate::screen_ask::trust_screen(&tail).is_some() || menu == Some(OpenMenu::Trust) {
        return Err(io::Error::other(t!(
            "它在等你决定要不要信任这个文件夹，答完才能发消息"
        )));
    }
    // Enter would pick 「Update now」 there, the option it starts on.
    if crate::screen_ask::update_screen(&tail).is_some() {
        return Err(io::Error::other(t!("它在问要不要更新，答完才能发消息")));
    }
    let rules = host
        .workspace
        .current_agent(id)
        .map(|s| s.runtime)
        .map_or_else(
            crate::screen_rules::default_rules,
            crate::screen_rules::rules,
        );
    if host.http_asks.contains_key(id)
        || menu == Some(OpenMenu::Approval)
        || crate::screen_rules::explain(rules, &tail)
            .approval()
            .is_some()
    {
        return Err(io::Error::other(t!(
            "它在等你审批，先允许或拒绝，再发送消息"
        )));
    }
    Ok(())
}
fn check_session(workspace: &Workspace, id: &str, expected: Option<&str>) -> io::Result<()> {
    if let Some(expected) = expected {
        if workspace.current_agent(id).map(|s| s.id.as_str()) != Some(expected) {
            return Err(io::Error::other(t!(
                "原对话已离开这个终端，消息未继续发送，请打开原对话处理"
            )));
        }
    }
    Ok(())
}
/// The bytes a key sends: arrows as the program asked for them (DECCKM), `ctrl-x` as
/// its control code, any other single character as itself.
pub(in crate::host) fn key_bytes(key: &str, application_cursor: bool) -> io::Result<Vec<u8>> {
    let arrow = |c: char| {
        let lead = if application_cursor { "\x1bO" } else { "\x1b[" };
        format!("{lead}{c}").into_bytes()
    };
    Ok(match key {
        "enter" => b"\r".to_vec(),
        "esc" => b"\x1b".to_vec(),
        "tab" => b"\t".to_vec(),
        "shift-tab" => b"\x1b[Z".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "space" => b" ".to_vec(),
        "up" => arrow('A'),
        "down" => arrow('B'),
        "right" => arrow('C'),
        "left" => arrow('D'),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "page-up" => b"\x1b[5~".to_vec(),
        "page-down" => b"\x1b[6~".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        _ => match (key.strip_prefix("ctrl-"), key.chars().count()) {
            (Some(c), _) if c.len() == 1 && c.as_bytes()[0].is_ascii_alphabetic() => {
                vec![c.as_bytes()[0].to_ascii_lowercase() & 0x1f]
            }
            (None, 1) if !key.chars().next().is_some_and(char::is_control) => {
                key.as_bytes().to_vec()
            }
            _ => return Err(io::Error::other(t!("不认识的按键 {key}", key))),
        },
    })
}
pub(in crate::host) fn send_keys(shared: &Shared, id: &str, keys: &[String]) -> io::Result<()> {
    if keys.is_empty() || keys.len() > 64 {
        return Err(io::Error::other(t!("一次发 1 到 64 个按键")));
    }
    let terminal = lock(shared)?
        .terminals
        .get(id)
        .cloned()
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    let (input, bytes) = {
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        let application_cursor = live
            .vt
            .terminal()
            .mode(fibo_core::terminal::libghostty_vt::terminal::Mode::DECCKM)
            .unwrap_or(false);
        // All or nothing: a typo in the last key sends none of them.
        let bytes = keys
            .iter()
            .map(|k| key_bytes(k, application_cursor))
            .collect::<io::Result<Vec<_>>>()?
            .concat();
        live.input_at = Some(Instant::now());
        (live.input.clone(), bytes)
    };
    input.write(bytes)?;
    typed(shared, id, keys.iter().any(|k| k == "enter"))
}
/// Raw input from another desktop's pane, written as it came.
pub(in crate::host) fn type_input(shared: &Shared, id: &str, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > 64 << 10 {
        return Err(io::Error::other(t!("一次发 1 字节到 64 KiB")));
    }
    let terminal = lock(shared)?
        .terminals
        .get(id)
        .cloned()
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    let input = {
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        live.input_at = Some(Instant::now());
        live.input.clone()
    };
    input.write(bytes)?;
    typed(shared, id, bytes.contains(&b'\r'))
}
/// How often the cursor is looked at again on the way to 「trust」, and how many times.
const TRUST_STEP: Duration = Duration::from_millis(400);
const TRUST_STEPS: usize = 6;

/// Answers an Agent's 「trust this folder?」 from what the screen says right now, each key
/// chosen from a fresh read: not trusting is Esc whatever the cursor is on; trusting
/// is Enter only once the cursor is seen on that option. A CLI just up can swallow an arrow,
/// so the arrow goes again if the cursor has not moved, and never a blind Enter.
pub(in crate::host) fn answer_trust(
    shared: &Shared,
    id: &str,
    folder: &str,
    trust: bool,
) -> io::Result<()> {
    let terminal = lock(shared)?
        .terminals
        .get(id)
        .cloned()
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    for _ in 0..TRUST_STEPS {
        let (input, bytes, done) = {
            let mut live = lock(&terminal)?;
            if live.exited {
                return Err(io::Error::other("terminal has exited"));
            }
            let tail = live.vt.tail_text(usize::from(live.rows))?;
            let seen = crate::screen_ask::trust_screen(&tail)
                .filter(|s| s.folder == folder)
                .ok_or_else(|| {
                    io::Error::other(t!("画面上已经没有这个信任提示了，可能已经答过"))
                })?;
            let bytes = match (trust, seen.on_yes) {
                (false, _) => b"\x1b".to_vec(),
                (true, true) => b"\r".to_vec(),
                (true, false) => {
                    let cursor = live
                        .vt
                        .terminal()
                        .mode(fibo_core::terminal::libghostty_vt::terminal::Mode::DECCKM)
                        .unwrap_or(false);
                    key_bytes(seen.toward_yes, cursor)?
                }
            };
            live.input_at = Some(Instant::now());
            (live.input.clone(), bytes, !trust || seen.on_yes)
        };
        input.write(bytes)?;
        // The card stops asking when the screen does, on its next quiet read: a key that
        // did nothing leaves the question there to answer again.
        if done {
            return Ok(());
        }
        thread::sleep(TRUST_STEP);
    }
    Err(io::Error::other(t!(
        "光标没有移到「信任」那一项，没有按回车，请打开终端处理"
    )))
}
/// The key its CLI interrupts a turn with, while the terminal's conversation is working.
/// The Agent stays and can be asked to go on, unlike `StopAgent`, which ends its process.
/// A 「Task is still running」 menu on screen is answered instead, read under the lock the
/// key goes out under: Enter only with the cursor on 「Cancel task」, since its other
/// options leave the task running or quit the Agent.
const PRESS_GAP_MS: u64 = 150;
pub(in crate::host) fn interrupt(shared: &Shared, id: &str) -> io::Result<()> {
    let w = lock(shared)?.snapshot();
    let session =
        (w.current_agent(id)).ok_or_else(|| io::Error::other(t!("这个终端里没有 Agent 对话")))?;
    let state = fibo_core::status::of(&w, session, fibo_core::content::now());
    if state.phase() != fibo_core::status::Phase::Working {
        return Err(io::Error::other(t!(
            "Agent 现在是「{label}」，没有要中断的",
            label = state.label()
        )));
    }
    let cli = crate::agents::adapter(session.runtime);
    let key = cli.interrupt_key();
    let mut presses = 1;
    let terminal = lock(shared)?
        .terminals
        .get(id)
        .cloned()
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    let input = {
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        let tail = live.vt.tail_text(usize::from(live.rows))?;
        let bytes: &[u8] = match crate::screen_ask::cancel_menu(&tail) {
            None => {
                presses = cli.interrupt_presses();
                key
            }
            Some(menu) if menu.on_cancel => b"\r",
            Some(_) => {
                return Err(io::Error::other(t!(
                    "画面上的菜单光标不在「Cancel task」上，没有替你选，请打开终端处理"
                )));
            }
        };
        live.input_at = Some(Instant::now());
        let input = live.input.clone();
        drop(live);
        input.write(bytes)?;
        input
    };
    // Apart, or a CLI reads them as one Alt-Esc; the lock stays free for its output.
    for _ in 1..presses {
        thread::sleep(Duration::from_millis(PRESS_GAP_MS));
        if !lock(&terminal)?.exited {
            input.write(key)?;
        }
    }
    typed(shared, id, false)
}
/// Same as typing in the app: the prompt on screen is no longer waiting on the user, and
/// Enter submits.
fn typed(shared: &Shared, id: &str, entered: bool) -> io::Result<()> {
    lock(shared)?.answer_screen_wait(id)?;
    if entered {
        record_submission(shared, id)?;
    }
    Ok(())
}

/// Enter for 允许一次, Esc for 拒绝, sent only while the screen still shows `question`
/// as an approval, read again under the same lock the key is written under: an approval
/// answered in the pane meanwhile gets nothing typed into what replaced it.
pub(in crate::host) fn answer_approval(
    shared: &Shared,
    id: &str,
    question: &str,
    allow: bool,
    always: bool,
) -> io::Result<()> {
    if let Some(done) = http_ask::answer(shared, id, question, allow, always) {
        return done;
    }
    let (terminal, rules) = {
        let host = lock(shared)?;
        let terminal = host
            .terminals
            .get(id)
            .cloned()
            .ok_or_else(|| io::Error::other("terminal not found"))?;
        let rules = host
            .workspace
            .current_agent(id)
            .map(|s| s.runtime)
            .map_or_else(
                crate::screen_rules::default_rules,
                crate::screen_rules::rules,
            );
        (terminal, rules)
    };
    {
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        let tail = live.vt.tail_text(usize::from(live.rows))?;
        let seen = crate::screen_rules::explain(rules, &tail);
        if seen.approval() != Some(question) {
            return Err(io::Error::other(t!(
                "画面上已经没有这个审批了，可能已经答过"
            )));
        }
        // Enter takes whichever option the cursor is on; only the first one is 允许一次.
        let key = if always {
            let key = seen.always_key();
            key.ok_or_else(|| io::Error::other(t!("这个审批没有「不再询问」这一项")))?
        } else if allow && !seen.cursor_on_first() {
            return Err(io::Error::other(t!("光标不在第一个选项上，请打开画面回答")));
        } else {
            (if allow { "\r" } else { "\x1b" }).to_owned()
        };
        live.input_at = Some(Instant::now());
        let input = live.input.clone();
        drop(live);
        input.write(key)?;
    }
    lock(shared)?.answer_screen_wait(id)
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use crate::host::tests::session;

    #[test]
    fn chat_identity_tracks_the_current_conversation_not_the_terminal() {
        let mut workspace = Workspace {
            agent_sessions: vec![session("a", 1, Some("t"), true)],
            ..Default::default()
        };
        assert!(check_session(&workspace, "t", Some("a")).is_ok());
        assert!(check_session(&workspace, "other", Some("a")).is_err());
        workspace.agent_sessions[0].current = false;
        workspace
            .agent_sessions
            .push(session("b", 2, Some("t"), true));
        assert!(check_session(&workspace, "t", Some("a")).is_err());
        assert!(check_session(&workspace, "t", Some("b")).is_ok());
        workspace.agent_sessions[1].current = false;
        assert!(check_session(&workspace, "t", Some("b")).is_err());
        assert!(check_session(&workspace, "t", Some("")).is_err());
        assert!(check_session(&workspace, "t", None).is_ok());
    }
}
