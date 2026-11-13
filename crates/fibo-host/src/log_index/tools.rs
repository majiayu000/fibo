//! A conversation's tool calls for a phone (`Request::ToolCalls`): which rows a stretch of
//! work holds and where their raw lines are. What the rows cut short is read from those
//! lines by the caller, away from this connection and the host's lock.
use super::chat::{place, summary, FAILED, LOG_INODE, SEQ};
use super::{err, Index};
use fibo_core::protocol::{ChatTools, ToolResultRow, ToolRow};
use rusqlite::{params, OptionalExtension, Row};
use std::io;

/// A page of calls: what the stretch adds up to (first page), its rows, the next cursor.
type Page = (Option<ChatTools>, Vec<ToolRow>, Option<i64>);

fn row(r: &Row) -> rusqlite::Result<ToolRow> {
    Ok(ToolRow {
        seq: r.get(0)?,
        id: r.get(1)?,
        text: r.get(2)?,
        at: u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
        inode: u64::try_from(r.get::<_, i64>(4)?).unwrap_or(0),
        offset: u64::try_from(r.get::<_, i64>(5)?).unwrap_or(0),
        results: Vec::new(),
    })
}
fn result(r: &Row) -> rusqlite::Result<ToolResultRow> {
    Ok(ToolResultRow {
        seq: r.get(0)?,
        at: u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
        code: r.get(2)?,
        inode: u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
        offset: u64::try_from(r.get::<_, i64>(4)?).unwrap_or(0),
    })
}
/// A call's results beyond this come from a command read back this many times; its first
/// and last stretches say what it did.
const MOST_RESULTS: usize = 32;

impl Index {
    /// The calls between the message before `seq` and `seq`, past the cursor `after`.
    pub fn tool_calls(
        &self,
        path: &str,
        seq: Option<i64>,
        after: Option<i64>,
        limit: usize,
    ) -> io::Result<Page> {
        let limit = limit.clamp(1, 200);
        let until = seq.unwrap_or(i64::MAX);
        let end = place(until);
        let from = self.stretch_start(path, end)?;
        let start = after.map_or(from, |a| a.max(from));
        let begin = place(start);
        let mut rows = self
            .db
            .prepare_cached(&format!("SELECT {SEQ},call_id,text,at,inode,offset FROM session_records WHERE path=?1 AND kind='tool_call' AND (offset,ordinal)>(?2,?3) AND (offset,ordinal)<(?4,?5) ORDER BY offset,ordinal LIMIT ?6"))
            .map_err(err)?
            .query_map(params![path, begin.0, begin.1, end.0, end.1, limit as i64 + 1], row)
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = more.then(|| rows.last().map_or(start, |r| r.seq));
        self.attach_results(path, &mut rows)?;
        let tools = match after {
            Some(_) => None,
            None => Some(self.stretch(path, from, until)?),
        };
        Ok((tools, rows, next))
    }

    /// The stretch's last `n` calls, oldest first, with what the whole stretch adds up to:
    /// what a running turn did just now, without paging through the hundreds before it.
    pub fn last_tool_calls(&self, path: &str, seq: Option<i64>, n: usize) -> io::Result<Page> {
        let until = seq.unwrap_or(i64::MAX);
        let end = place(until);
        let from = self.stretch_start(path, end)?;
        let begin = place(from);
        let mut rows = self
            .db
            .prepare_cached(&format!("SELECT {SEQ},call_id,text,at,inode,offset FROM session_records WHERE path=?1 AND kind='tool_call' AND (offset,ordinal)>(?2,?3) AND (offset,ordinal)<(?4,?5) ORDER BY offset DESC,ordinal DESC LIMIT ?6"))
            .map_err(err)?
            .query_map(params![path, begin.0, begin.1, end.0, end.1, n.clamp(1, 20) as i64], row)
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        rows.reverse();
        self.attach_results(path, &mut rows)?;
        Ok((Some(self.stretch(path, from, until)?), rows, None))
    }

    /// Where the stretch ending at `end` begins: the Agent's message before it. A message you
    /// send while it works lands mid-stretch, between its calls; starting there lost the calls
    /// before it from every count (2026-11-10 walk). The `IN` lets SQLite walk the
    /// messages' partial index instead of every record. A turn stopped or refused after that
    /// message ends with no reply; the stretch starts at its interruption, or the stopped
    /// calls counted on the next reply (2026-11-12 walk). Looking only past the Agent's
    /// message keeps the search to this stretch.
    pub(super) fn stretch_start(&self, path: &str, end: (i64, i64)) -> io::Result<i64> {
        let said: i64 = self
            .db
            .prepare_cached(&format!("SELECT {SEQ} FROM session_records WHERE path=?1 AND inode={LOG_INODE} AND kind IN ('user','assistant') AND kind='assistant' AND (offset,ordinal)<(?2,?3) ORDER BY offset DESC,ordinal DESC LIMIT 1"))
            .map_err(err)?
            .query_row(params![path, end.0, end.1], |r| r.get(0))
            .optional()
            .map_err(err)?
            .unwrap_or(0);
        Ok(self
            .interruptions(path, place(said), end)?
            .pop()
            .unwrap_or(said))
    }

    /// Where turns were stopped between two places, in order.
    pub(super) fn interruptions(
        &self,
        path: &str,
        from: (i64, i64),
        until: (i64, i64),
    ) -> io::Result<Vec<i64>> {
        self.db
            .prepare_cached(&format!("SELECT {SEQ} FROM session_records WHERE path=?1 AND inode={LOG_INODE} AND (offset,ordinal)>(?2,?3) AND (offset,ordinal)<(?4,?5) AND kind='turn' AND text='Interrupted' ORDER BY offset,ordinal"))
            .map_err(err)?
            .query_map(params![path, from.0, from.1, until.0, until.1], |r| r.get(0))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)
    }

    /// Which log the turn begun at `since` is in and where its opening message is, so reading
    /// it costs what the turn wrote and not what the conversation did before. `None`: the
    /// log has no message that old.
    pub fn turn_start(&self, path: &str, since: u64) -> io::Result<Option<(u64, u64)>> {
        self.db
            .prepare_cached(&format!("SELECT inode,offset FROM session_records WHERE path=?1 AND inode={LOG_INODE} AND kind='user' AND at<=?2 ORDER BY offset DESC,ordinal DESC LIMIT 1"))
            .map_err(err)?
            .query_row(params![path, since as i64], |r| {
                Ok((
                    u64::try_from(r.get::<_, i64>(0)?).unwrap_or(0),
                    u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                ))
            })
            .optional()
            .map_err(err)
    }

    /// One call by its id, wherever in the conversation it is.
    pub fn tool_call(&self, path: &str, id: &str) -> io::Result<Option<ToolRow>> {
        let Some(mut found) = self
            .db
            .prepare_cached(&format!("SELECT {SEQ},call_id,text,at,inode,offset FROM session_records WHERE path=?1 AND call_id=?2 AND kind='tool_call' ORDER BY offset,ordinal LIMIT 1"))
            .map_err(err)?
            .query_row(params![path, id], row)
            .optional()
            .map_err(err)?
        else {
            return Ok(None);
        };
        found.results = self
            .db
            .prepare_cached(&format!("SELECT {SEQ},at,exit_code,inode,offset FROM session_records WHERE path=?1 AND call_id=?2 AND kind='tool_result' ORDER BY offset,ordinal LIMIT {MOST_RESULTS}"))
            .map_err(err)?
            .query_map(params![path, id], result)
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(Some(found))
    }

    /// Each row's results, which may lie past the page.
    fn attach_results(&self, path: &str, rows: &mut [ToolRow]) -> io::Result<()> {
        let Some(first) = rows.first().map(|r| r.seq) else {
            return Ok(());
        };
        let from = place(first);
        let mut q = self
            .db
            .prepare_cached(&format!("SELECT {SEQ},at,exit_code,inode,offset FROM session_records WHERE path=?1 AND call_id=?2 AND kind='tool_result' AND (offset,ordinal)>(?3,?4) ORDER BY offset,ordinal LIMIT {MOST_RESULTS}"))
            .map_err(err)?;
        for r in rows {
            r.results = q
                .query_map(params![path, r.id, from.0, from.1], result)
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
        }
        Ok(())
    }

    /// What a whole stretch adds up to, as `chat`'s `tools` counts it.
    fn stretch(&self, path: &str, from: i64, until: i64) -> io::Result<ChatTools> {
        let (from, until) = (place(from), place(until));
        let calls = self
            .db
            .prepare_cached(&format!("SELECT text,{FAILED} FROM session_records AS call WHERE path=?1 AND kind='tool_call' AND (offset,ordinal)>(?2,?3) AND (offset,ordinal)<(?4,?5) ORDER BY offset,ordinal"))
            .map_err(err)?
            .query_map(params![path, from.0, from.1, until.0, until.1], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(summary(&calls))
    }
}
