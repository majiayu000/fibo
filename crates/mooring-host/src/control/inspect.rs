//! `mooring events`, `list` and `read`: what the host and its terminals show now.
use super::*;

/// Event subscription: the state now, then one line per change until the host
/// goes away or the reader stops reading.
pub(super) fn events(a: Args) -> io::Result<()> {
    let mut stream = std::os::unix::net::UnixStream::connect(&a.socket)?;
    mooring_core::protocol::send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
    match receive::<Reply>(&mut stream)? {
        Reply::Hello { major: MAJOR, .. } => (),
        r => return Err(io::Error::other(format!("handshake: {r:?}"))),
    }
    mooring_core::protocol::send(&mut stream, CONTROL, &Request::Subscribe)?;
    let json = a.has("json");
    loop {
        let notices = match receive::<Reply>(&mut stream)? {
            Reply::Events(n) => n,
            Reply::Error(e) => return Err(io::Error::other(e)),
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        };
        for n in notices {
            if json {
                println!("{}", serde_json::to_string(&n).map_err(io::Error::other)?);
                continue;
            }
            let terminal: String = n
                .terminal
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(8)
                .collect();
            let what = [n.title.as_str(), n.detail.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let code = n
                .exit_code
                .map(|c| format!(" 退出码 {c}"))
                .unwrap_or_default();
            println!("{:<16} {terminal:<8} {what}{code}", n.event);
        }
    }
}
type Row<'a> = (&'a Terminal, Option<&'a AgentSession>, Phase, String);
fn list_rows(w: &Workspace) -> Vec<Row<'_>> {
    w.terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        .map(|t| {
            let s = w.current_agent(&t.id);
            (t, s, phase(w, &t.id), tab_title(w, &t.id))
        })
        .collect()
}
/// `mooring list --json`: every field read from its one owner in core.
pub(super) fn list_json(w: &Workspace) -> Vec<serde_json::Value> {
    list_rows(w)
        .iter()
        .map(|(t, s, phase, tab)| {
            serde_json::json!({
                "id": t.id, "cwd": w.terminal_cwd(t), "tab": tab,
                "agent": s.map(|s| agent_label(s.runtime)),
                "phase": phase.name(),
                "status": status(w, &t.id).map(Status::key),
                "status_label": status(w, &t.id).map(Status::label),
                "title": s.map(|s| w.session_title(s)),
                "session": s.map(|s| &s.cli_session_id),
            })
        })
        .collect()
}
pub(super) fn list(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let rows = list_rows(&w);
    if a.has("json") {
        let json = list_json(&w);
        println!(
            "{}",
            serde_json::to_string_pretty(&json).map_err(io::Error::other)?
        );
        return Ok(());
    }
    // Full ids: terminals opened by one app share a long prefix, so a cut-off id names none.
    let id_width = rows.iter().map(|r| r.0.id.len()).max().unwrap_or(0);
    let tab_width = rows
        .iter()
        .map(|r| r.3.chars().count().max(1))
        .max()
        .unwrap_or(1);
    for (t, s, phase, tab) in rows {
        let agent = s.map_or_else(|| "-".into(), |s| agent_label(s.runtime));
        let tab = if tab.is_empty() { "-".to_owned() } else { tab };
        let pad = " ".repeat(tab_width - tab.chars().count());
        println!(
            "{:<id_width$}  {:<6}  {:<7}  {tab}{pad}  {}",
            t.id,
            agent,
            phase.name(),
            w.terminal_cwd(t)
        );
    }
    Ok(())
}
pub(super) fn read(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, "终端")?)?;
    let rows = a.number("lines", 50)? as usize;
    match rpc(
        &a.socket,
        &Request::ReadScreen {
            id,
            rows: rows + 200,
        },
    )? {
        Reply::Text(text) => {
            // The screen's empty rows below the cursor say nothing.
            let lines: Vec<_> = text.trim_end().lines().collect();
            println!("{}", lines[lines.len().saturating_sub(rows)..].join("\n"));
            Ok(())
        }
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
