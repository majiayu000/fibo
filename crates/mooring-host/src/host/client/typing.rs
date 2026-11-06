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
    if text.contains('\x1b') {
        return Err(io::Error::other("文本包含终端控制字符"));
    }
    if text.len() > 64 * 1024 {
        return Err(io::Error::other("单次输入超过 64 KiB"));
    }
    let host = lock(shared)?;
    check_session(&host.workspace, id, session)?;
    let terminal = host
        .terminals
        .get(id)
        .cloned()
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    {
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        if !text.is_empty() {
            let bracketed = live
                .vt
                .terminal()
                .mode(mooring_core::terminal::libghostty_vt::terminal::Mode::BRACKETED_PASTE)
                .unwrap_or(false);
            let bytes = if bracketed {
                format!("\x1b[200~{text}\x1b[201~")
            } else if text.contains(['\n', '\r']) {
                return Err(io::Error::other("程序未启用安全粘贴，不能发送多行文本"));
            } else {
                text.to_owned()
            };
            pty::write(&mut live.master, bytes.as_bytes())?;
            live.input_at = Some(Instant::now());
        }
    }
    drop(host);
    // Same as typing in the app: the prompt on screen is no longer waiting on the user.
    lock(shared)?.set_screen_wait(id, None)?;
    if !submit {
        return Ok(());
    }
    // Long enough for an Agent's paste handling to finish before Enter arrives.
    thread::sleep(Duration::from_millis(150));
    {
        let host = lock(shared)?;
        check_session(&host.workspace, id, session)?;
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        pty::write(&mut live.master, b"\r")?;
    }
    record_submission(shared, id)
}
fn check_session(workspace: &Workspace, id: &str, expected: Option<&str>) -> io::Result<()> {
    if let Some(expected) = expected {
        if workspace.current_agent(id).map(|s| s.id.as_str()) != Some(expected) {
            return Err(io::Error::other(
                "原对话已离开这个终端，消息未继续发送，请打开原对话处理",
            ));
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
            _ => return Err(io::Error::other(format!("不认识的按键 {key}"))),
        },
    })
}
pub(in crate::host) fn send_keys(shared: &Shared, id: &str, keys: &[String]) -> io::Result<()> {
    if keys.is_empty() || keys.len() > 64 {
        return Err(io::Error::other("一次发 1 到 64 个按键"));
    }
    let terminal = lock(shared)?
        .terminals
        .get(id)
        .cloned()
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    {
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        let application_cursor = live
            .vt
            .terminal()
            .mode(mooring_core::terminal::libghostty_vt::terminal::Mode::DECCKM)
            .unwrap_or(false);
        // All or nothing: a typo in the last key sends none of them.
        let bytes = keys
            .iter()
            .map(|k| key_bytes(k, application_cursor))
            .collect::<io::Result<Vec<_>>>()?
            .concat();
        pty::write(&mut live.master, &bytes)?;
        live.input_at = Some(Instant::now());
    }
    typed(shared, id, keys.iter().any(|k| k == "enter"))
}
/// Raw input from another desktop's pane, written as it came.
pub(in crate::host) fn type_input(shared: &Shared, id: &str, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > 64 << 10 {
        return Err(io::Error::other("一次发 1 字节到 64 KiB"));
    }
    let terminal = lock(shared)?
        .terminals
        .get(id)
        .cloned()
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    {
        let mut live = lock(&terminal)?;
        if live.exited {
            return Err(io::Error::other("terminal has exited"));
        }
        pty::write(&mut live.master, bytes)?;
        live.input_at = Some(Instant::now());
    }
    typed(shared, id, bytes.contains(&b'\r'))
}
/// How often the cursor is looked at again on the way to 「trust」, and how many times.
const TRUST_STEP: Duration = Duration::from_millis(400);
const TRUST_STEPS: usize = 6;

/// Answers an Agent's 「trust this folder?」 from what the screen says right now, read under
/// the lock each key goes out under: not trusting is Esc whatever the cursor is on; trusting
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
        let done = {
            let mut live = lock(&terminal)?;
            if live.exited {
                return Err(io::Error::other("terminal has exited"));
            }
            let tail = live.vt.tail_text(usize::from(live.rows))?;
            let seen = crate::screen_ask::trust_screen(&tail)
                .filter(|s| s.folder == folder)
                .ok_or_else(|| io::Error::other("画面上已经没有这个信任提示了，可能已经答过"))?;
            let bytes = match (trust, seen.on_yes) {
                (false, _) => b"\x1b".to_vec(),
                (true, true) => b"\r".to_vec(),
                (true, false) => {
                    let cursor = live
                        .vt
                        .terminal()
                        .mode(mooring_core::terminal::libghostty_vt::terminal::Mode::DECCKM)
                        .unwrap_or(false);
                    key_bytes(seen.toward_yes, cursor)?
                }
            };
            pty::write(&mut live.master, &bytes)?;
            live.input_at = Some(Instant::now());
            !trust || seen.on_yes
        };
        // The card stops asking when the screen does, on its next quiet read: a key that
        // did nothing leaves the question there to answer again.
        if done {
            return Ok(());
        }
        thread::sleep(TRUST_STEP);
    }
    Err(io::Error::other(
        "光标没有移到「信任」那一项，没有按回车，请打开终端处理",
    ))
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
        (w.current_agent(id)).ok_or_else(|| io::Error::other("这个终端里没有 Agent 对话"))?;
    let state = mooring_core::status::of(&w, session, mooring_core::content::now());
    if state.phase() != mooring_core::status::Phase::Working {
        return Err(io::Error::other(format!(
            "Agent 现在是「{}」，没有要中断的",
            state.label()
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
    {
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
                return Err(io::Error::other(
                    "画面上的菜单光标不在「Cancel task」上，没有替你选，请打开终端处理",
                ))
            }
        };
        pty::write(&mut live.master, bytes)?;
        live.input_at = Some(Instant::now());
    }
    // Apart, or a CLI reads them as one Alt-Esc; the lock stays free for its output.
    for _ in 1..presses {
        thread::sleep(Duration::from_millis(PRESS_GAP_MS));
        let mut live = lock(&terminal)?;
        if !live.exited {
            pty::write(&mut live.master, key)?;
        }
    }
    typed(shared, id, false)
}
/// Same as typing in the app: the prompt on screen is no longer waiting on the user, and
/// Enter submits.
fn typed(shared: &Shared, id: &str, entered: bool) -> io::Result<()> {
    lock(shared)?.set_screen_wait(id, None)?;
    if entered {
        record_submission(shared, id)?;
    }
    Ok(())
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
