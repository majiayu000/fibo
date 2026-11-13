//! `Request::InputSuggestion`: read when a phone asks, under the locks for a moment, so a
//! terminal's output and the window pay nothing for it.
use super::super::{lock, Shared};
use mooring_core::protocol::Reply;
use std::io;

pub(super) fn read(shared: &Shared, id: &str) -> io::Result<Reply> {
    let host = lock(shared)?;
    let terminal = host
        .terminals
        .get(id)
        .ok_or_else(|| io::Error::other("terminal not found"))?;
    let Some(session) = host.workspace.current_agent(id) else {
        return Ok(Reply::InputSuggestion {
            session: None,
            text: None,
        });
    };
    let text = match crate::agents::adapter(session.runtime).input_suggestion() {
        Some(read) => read(&lock(terminal)?.vt)?,
        None => None,
    };
    Ok(Reply::InputSuggestion {
        session: Some(session.id.clone()),
        text,
    })
}
