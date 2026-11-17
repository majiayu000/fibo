//! Looking for the Agent this host just started in a terminal (`Request::Launch` with a
//! `start`). Nothing else would: its first message came on the command line, so no Enter
//! announces it, and a conversation it resumed may write nothing to its log until it is
//! spoken to, so no log event does either. A few looks while it starts, then none.
use super::*;

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

/// Looks until the terminal runs a conversation, or its Agent is seen there (its log will
/// tie it from then on), or it ends; at most `LOOKS.len()` times.
pub(super) fn look_for_started(shared: &Shared, terminal: &str) {
    let (shared, terminal) = (shared.clone(), terminal.to_owned());
    thread::spawn(move || {
        for wait in LOOKS {
            thread::sleep(wait);
            let again = (|| -> io::Result<bool> {
                let transcripts = {
                    let host = lock(&shared)?;
                    if !waiting(&host, &terminal) {
                        return Ok(false);
                    }
                    host.index.cached()?
                };
                refresh_agents(&shared, &transcripts)?;
                let host = lock(&shared)?;
                Ok(waiting(&host, &terminal)
                    && !host.recognition.terminal_agents.contains_key(&terminal))
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

/// Started with an Agent, and nothing tied to a conversation yet.
fn waiting(host: &Host, terminal: &str) -> bool {
    host.workspace.agent_starts.contains_key(terminal)
        && host.workspace.current_agent(terminal).is_none()
}
