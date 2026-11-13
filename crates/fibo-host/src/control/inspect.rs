//! `fibo events`, `list` and `read`: what the host and its terminals show now.
use super::*;

/// Event subscription: the state now, then one line per change until the host
/// goes away or the reader stops reading.
pub(super) fn events(a: Args) -> io::Result<()> {
    let mut stream = fibo_platform::local::Stream::connect(&a.socket)?;
    fibo_core::protocol::send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
    match receive::<Reply>(&mut stream)? {
        Reply::Hello { major: MAJOR, .. } => (),
        r => return Err(io::Error::other(format!("handshake: {r:?}"))),
    }
    fibo_core::protocol::send(&mut stream, CONTROL, &Request::Subscribe)?;
    let json = a.has("json");
    // Tab titles by terminal id, read again only for a terminal not seen yet: a starting terminal's notice has none.
    let mut tabs: std::collections::HashMap<String, String> = Default::default();
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
            let id = n.terminal.as_deref().unwrap_or("");
            if !id.is_empty() && !tabs.contains_key(id) {
                // One read fills every terminal, so the opening snapshot costs one.
                if let Ok(w) = workspace(&a.socket) {
                    for t in &w.terminals {
                        tabs.insert(t.id.clone(), tab_title(&w, &t.id));
                    }
                }
                tabs.entry(id.to_owned()).or_default();
            }
            let tab = tabs.get(id).map_or("", String::as_str);
            println!("{}", super::events_text::line(&n, tab));
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
/// `fibo list --json`: every field read from its one owner in core.
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
    let lines = rows
        .into_iter()
        .map(|(t, s, _, tab)| super::list_text::Line {
            id: t.id.clone(),
            agent: s.map(|s| agent_label(s.runtime)),
            status: status(&w, &t.id),
            title: tab,
            cwd: w.terminal_cwd(t).to_string(),
        })
        .collect();
    let home = fibo_platform::dirs::home()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let color = {
        use std::io::IsTerminal;
        std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
    };
    print!("{}", super::list_text::render(lines, &home, color));
    Ok(())
}
pub(super) fn read(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, t!("终端"))?)?;
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
