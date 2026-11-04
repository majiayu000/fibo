//! Text, named keys and raw input typed into a terminal from outside its pane.
use super::*;

/// A paste from outside the app, plus Enter as its own write: an Agent that sees the
/// Enter inside the paste would take it as a newline in the prompt, not a submit.
pub(in crate::host) fn send_text(
    shared: &Shared,
    id: &str,
    text: &str,
    submit: bool,
) -> io::Result<()> {
    if text.contains('\x1b') {
        return Err(io::Error::other("文本包含终端控制字符"));
    }
    if text.len() > 64 * 1024 {
        return Err(io::Error::other("单次输入超过 64 KiB"));
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
    // Same as typing in the app: the prompt on screen is no longer waiting on the user.
    lock(shared)?.set_screen_wait(id, None)?;
    if !submit {
        return Ok(());
    }
    // Long enough for an Agent's paste handling to finish before Enter arrives.
    thread::sleep(Duration::from_millis(150));
    pty::write(&mut lock(&terminal)?.master, b"\r")?;
    record_submission(shared, id)
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
/// Ctrl-C (byte 3), the window's 中断, while the terminal's conversation is working. The
/// Agent stays and can be asked to go on, unlike `StopAgent`, which ends its process.
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
    type_input(shared, id, &[3])
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
