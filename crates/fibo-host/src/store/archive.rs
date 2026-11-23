//! Rows the host left out of memory (`fibo_core::retention`), read back one at a time when
//! their conversation moves again. Loading reads the same rows the same way.
use super::{db_error, enum_value, Store};
use fibo_core::model::{Agent, AgentSession, Outcome, Terminal};
use rusqlite::{params, OptionalExtension, Row};
use std::io;

pub(super) const SESSIONS: &str = "SELECT a.id,a.runtime,a.cli_session_id,a.terminal_id,a.is_current,a.task_id,a.outcome,a.deferred_reason,a.deferred_until,a.updated,a.cwd,a.transcript,b.evidence FROM agent_sessions a LEFT JOIN agent_bindings b ON b.session_id=a.id";
pub(super) const TERMINALS: &str =
    "SELECT id,space_id,argv,cwd,pid,process_state,exit_code,error FROM terminals";

/// A row of `SESSIONS`, without its name (kept in `session_names`).
pub(super) fn session(r: &Row) -> io::Result<AgentSession> {
    let kind: String = r.get(6).map_err(db_error)?;
    let outcome = match kind.as_str() {
        "Open" => Outcome::Open,
        "Completed" => Outcome::Completed,
        "Abandoned" => Outcome::Abandoned,
        "Deferred" => Outcome::Deferred {
            reason: r.get(7).map_err(db_error)?,
            until: u64::try_from(r.get::<_, i64>(8).map_err(db_error)?).map_err(db_error)?,
        },
        _ => return Err(io::Error::other("unknown stored outcome")),
    };
    Ok(AgentSession {
        id: r.get(0).map_err(db_error)?,
        runtime: enum_value(r.get(1).map_err(db_error)?)?,
        cli_session_id: r.get(2).map_err(db_error)?,
        terminal: r.get(3).map_err(db_error)?,
        current: r.get(4).map_err(db_error)?,
        task: r.get(5).map_err(db_error)?,
        outcome,
        updated: u64::try_from(r.get::<_, i64>(9).map_err(db_error)?).map_err(db_error)?,
        cwd: r.get(10).map_err(db_error)?,
        transcript: r.get(11).map_err(db_error)?,
        binding: r
            .get::<_, Option<String>>(12)
            .map_err(db_error)?
            .map(enum_value)
            .transpose()?,
        name: String::new(),
    })
}

/// A row of `TERMINALS`.
pub(super) fn terminal(r: &Row) -> io::Result<Terminal> {
    let argv: String = r.get(2).map_err(db_error)?;
    Ok(Terminal {
        id: r.get(0).map_err(db_error)?,
        space: r.get(1).map_err(db_error)?,
        argv: serde_json::from_str(&argv).map_err(db_error)?,
        cwd: r.get(3).map_err(db_error)?,
        pid: r.get(4).map_err(db_error)?,
        state: enum_value(r.get(5).map_err(db_error)?)?,
        exit_code: r.get(6).map_err(db_error)?,
        error: r.get(7).map_err(db_error)?,
    })
}

impl Store {
    /// The stored row of this CLI conversation, with its name.
    pub fn stored_session(&self, runtime: Agent, cli: &str) -> io::Result<Option<AgentSession>> {
        let sql = format!("{SESSIONS} WHERE a.runtime=?1 AND a.cli_session_id=?2");
        let runtime = super::enum_name(&runtime)?;
        let Some(mut s) = self
            .0
            .query_row(&sql, params![runtime, cli], |r| Ok(session(r)))
            .optional()
            .map_err(db_error)?
            .transpose()?
        else {
            return Ok(None);
        };
        s.name = self
            .0
            .query_row(
                "SELECT name FROM session_names WHERE session_id=?1",
                [&s.id],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_error)?
            .unwrap_or_default();
        Ok(Some(s))
    }
    pub fn stored_terminal(&self, id: &str) -> io::Result<Option<Terminal>> {
        self.0
            .query_row(&format!("{TERMINALS} WHERE id=?1"), [id], |r| {
                Ok(terminal(r))
            })
            .optional()
            .map_err(db_error)?
            .transpose()
    }
}
