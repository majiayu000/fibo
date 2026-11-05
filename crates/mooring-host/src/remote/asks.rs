//! `ask`: what a screen asks of its user, read here so a phone never parses a terminal.
use super::text;
use mooring_core::protocol::*;
use mooring_host::{screen_ask, screen_rules};
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// Rows read for a prompt: a long command stands far above its question.
const ROWS: usize = 400;

fn screen(socket: &Path, terminal: &str) -> io::Result<String> {
    let read = Request::ReadScreen {
        id: terminal.to_owned(),
        rows: ROWS,
    };
    match rpc(socket, &read)? {
        Reply::Text(t) => Ok(t),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

fn read_ask(socket: &Path, terminal: &str) -> io::Result<rp::Ask> {
    let tail = screen(socket, terminal)?;
    // The same reading that made the card's `approval`: a question scrolled up is none.
    let seen = screen_rules::explain(screen_rules::default_rules(), &tail);
    let question = (seen.approval())
        .ok_or_else(|| io::Error::other("画面上现在没有审批"))?
        .to_owned();
    let a = screen_ask::approval_ask(&tail).unwrap_or_default();
    Ok(rp::Ask {
        question,
        kind: a.kind,
        subject: a.subject,
        note: a.note,
        detail: a.detail,
        truncated: a.truncated,
    })
}

pub(super) fn ask(socket: &Path, terminal: &str) -> io::Result<Value> {
    Ok(json!(read_ask(socket, terminal)?))
}

/// An `answer` carrying the `subject` its client showed answers that one or nothing: the
/// question is the same words for every command, and the Agent may have moved on since.
pub(super) fn same_subject(socket: &Path, terminal: &str, request: &Value) -> io::Result<()> {
    if request["subject"].is_null() {
        return Ok(());
    }
    let showed = text(request, "subject")?;
    let now = read_ask(socket, terminal)?;
    if now.subject != showed {
        return Err(io::Error::other("画面上的审批已经换了一条，请重新看一眼"));
    }
    Ok(())
}
