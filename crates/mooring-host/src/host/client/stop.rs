//! How an interrupted turn ends: the second half of the window's 中断这一轮 and a phone's 停下.
use super::*;

/// How long an interrupted turn gets to end by itself before the command it left is closed:
/// one with nothing running ends at once, and a server from an earlier turn lives on.
const BACKGROUND_AFTER: Duration = Duration::from_millis(1500);
/// How long the turn is watched: under a client's request limit, long enough for a log to
/// say so. A real Codex and Claude Code both settled within 3 s.
const STOP_WAIT: Duration = Duration::from_secs(7);

/// Waits for the state to say the turn ended, with the Agent still there, and otherwise says
/// what it saw. Pressing again here would race the screen: a second key can land on whatever
/// replaced the menu the first one answered. A command the turn left running is part of what
/// was asked to stop; the reply says when it closed one.
pub(super) fn await_interrupt(shared: &Shared, id: &str) -> io::Result<Reply> {
    let began = Instant::now();
    let until = began + STOP_WAIT;
    let mut closed = false;
    loop {
        let w = lock(shared)?.snapshot();
        let Some(session) = w.current_agent(id) else {
            return Err(io::Error::other("Agent 退出了，这个终端里已经没有对话"));
        };
        let state = mooring_core::status::of(&w, session, mooring_core::content::now());
        if state.phase() != mooring_core::status::Phase::Working {
            return Ok(Reply::Interrupted {
                closed_background: closed,
            });
        }
        if !closed && began.elapsed() >= BACKGROUND_AFTER {
            let cli = crate::agents::adapter(session.runtime);
            let read = Request::ReadScreen {
                id: id.to_owned(),
                rows: 30,
            };
            if let Reply::Text(tail) = dispatch(shared, read)? {
                if let Some(text) = cli.closes_background(&tail) {
                    send_text(shared, id, text, true, None)?;
                    closed = true;
                }
            }
        }
        if Instant::now() >= until {
            return Err(io::Error::other(
                "发了中断，但它没有停。可以打开终端看一眼，或者再试一次",
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
