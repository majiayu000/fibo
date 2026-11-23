//! Looking for the Agent this host just started in a terminal (`Request::Launch` with a
//! `start`), or one an older host started that still waits when its terminal is handed over.
//! Nothing else would: its first message came on the command line, so no Enter announces it,
//! and a conversation it resumed may write nothing to its log until it is spoken to, so no log
//! event does either. A few looks while it starts, then none.
use super::*;
use fibo_core::terminal::libghostty_vt::{screen::Screen, terminal::Mode};

/// How long to wait before each look: about a minute in all, since a login shell and a CLI
/// can take tens of seconds to start on a loaded machine.
const LOOKS: [Duration; 7] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(15),
    Duration::from_secs(30),
];
/// How often the screen of a resumed CLI is looked at for its input, and for how long.
const INPUT_EVERY: Duration = Duration::from_millis(200);
const INPUT_FOR: Duration = Duration::from_secs(120);
/// The least time between two process looks while its input shows but nothing tied it.
const TIE_EVERY: Duration = Duration::from_secs(2);

/// Looks until the terminal runs a conversation, or its Agent is seen there (its log will
/// tie it from then on), or it ends; at most `LOOKS.len()` times. An Agent resuming a
/// conversation is tied once it shows its input, which nothing announces either: its
/// screen is watched for that.
pub(super) fn look_for_started(shared: &Shared, terminal: &str) {
    let (shared, terminal) = (shared.clone(), terminal.to_owned());
    thread::spawn(move || {
        for wait in LOOKS {
            thread::sleep(wait);
            let again = (|| -> io::Result<bool> {
                if !look(&shared, &terminal)? {
                    return Ok(false);
                }
                let host = lock(&shared)?;
                if !host.recognition.terminal_agents.contains_key(&terminal) {
                    return Ok(true);
                }
                let resuming = (host.workspace.agent_starts.get(&terminal))
                    .is_some_and(|s| s.resume.is_some());
                drop(host);
                if resuming {
                    watch_input(&shared, &terminal)?;
                }
                Ok(false)
            })();
            match again {
                Ok(true) => (),
                Ok(false) => return,
                Err(e) => {
                    eprintln!("terminal {terminal}: looking for the Agent it started: {e}");
                    return;
                }
            }
        }
    });
}

/// One process look, if the terminal still waits; whether it still does after.
fn look(shared: &Shared, terminal: &str) -> io::Result<bool> {
    let transcripts = {
        let host = lock(shared)?;
        if !waiting(&host, terminal) {
            return Ok(false);
        }
        host.transcripts.clone()
    };
    refresh_agents(shared, &super::status::all_transcripts(&transcripts)?)?;
    let host = lock(shared)?;
    Ok(waiting(&host, terminal))
}

/// Until the resumed CLI shows its input and that ties the terminal, or it stops waiting:
/// the screen every `INPUT_EVERY`, a process look once it shows.
fn watch_input(shared: &Shared, terminal: &str) -> io::Result<()> {
    let until = Instant::now() + INPUT_FOR;
    let mut tried: Option<Instant> = None;
    while Instant::now() < until {
        thread::sleep(INPUT_EVERY);
        let shown = {
            let host = lock(shared)?;
            if !waiting(&host, terminal) {
                return Ok(());
            }
            shown(&host).contains(terminal)
        };
        if shown && tried.is_none_or(|at| at.elapsed() >= TIE_EVERY) {
            tried = Some(Instant::now());
            if !look(shared, terminal)? {
                return Ok(());
            }
        }
    }
    Ok(())
}

/// Started with an Agent, and nothing tied to a conversation yet.
fn waiting(host: &Host, terminal: &str) -> bool {
    host.workspace.agent_starts.contains_key(terminal)
        && host.workspace.current_agent(terminal).is_none()
}

/// The terminals waiting on a conversation they resume whose CLI shows its input now, for
/// `recognition::launched::resume`.
pub(super) fn shown(host: &Host) -> std::collections::HashSet<String> {
    (host.workspace.agent_starts.iter())
        .filter(|(id, s)| s.resume.is_some() && host.workspace.current_agent(id).is_none())
        .filter(|(id, s)| {
            host.terminals.get(*id).is_some_and(|live| {
                lock(live).is_ok_and(|live| input_shown(&live, s.agent).unwrap_or(false))
            })
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// Whether `live` shows its CLI's input, ready for a message: a full-screen TUI on the
/// alternate screen with the terminal's cursor showing, or an input box `Agent::input_ready`
/// knows. OpenCode 1.19.5 hides the cursor before it turns on the alternate screen and shows
/// it only while its prompt is focused (opentui's textarea `renderCursor`); what is typed
/// before that is dropped. Measured 2026-11-17 on its home and session screens:
/// `rc4/fix-r1/tui-ready-*.jsonl`.
pub(super) fn input_shown(live: &Live, agent: Agent) -> io::Result<bool> {
    let t = live.vt.terminal();
    if t.active_screen().map_err(io::Error::other)? == Screen::Alternate
        && t.mode(Mode::CURSOR_VISIBLE).map_err(io::Error::other)?
    {
        return Ok(true);
    }
    Ok(agent.input_ready(&live.vt.tail_text(usize::from(live.rows))?))
}

/// The CLI `pid` the host started resuming a conversation in `terminal` has exited: if it
/// tied nothing, its start is over. A CLI started there later by hand resumes nothing the
/// host knows of.
pub(super) fn cli_exited(host: &mut Host, pid: u32) -> io::Result<()> {
    let Some(terminal) = host.started_clis.remove(&pid) else {
        return Ok(());
    };
    if !waiting(host, &terminal) || host.started_clis.values().any(|t| *t == terminal) {
        return Ok(());
    }
    let mut next = host.workspace.clone();
    next.agent_starts.remove(&terminal);
    host.commit(next)
}
