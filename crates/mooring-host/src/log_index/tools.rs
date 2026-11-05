//! A conversation's tool calls for a phone (`Request::ToolCalls`): which rows a stretch of
//! work holds and where their raw lines are. What the rows cut short is read from those
//! lines by the caller, away from this connection and the host's lock.
use super::chat::summary;
use super::{err, Index};
use mooring_core::protocol::{ChatTools, ToolResultRow, ToolRow};
use rusqlite::{params, OptionalExtension, Row};
use std::collections::{HashMap, HashSet};
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
const RESULT: &str = "SELECT rowid,at,exit_code,inode,offset";
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
        let from: i64 = self
            .db
            .prepare_cached("SELECT rowid FROM session_records WHERE path=?1 AND kind IN ('user','assistant') AND rowid<?2 ORDER BY rowid DESC LIMIT 1")
            .map_err(err)?
            .query_row(params![path, until], |r| r.get(0))
            .optional()
            .map_err(err)?
            .unwrap_or(0);
        let start = after.map_or(from, |a| a.max(from));
        let mut rows = self
            .db
            .prepare_cached("SELECT rowid,call_id,text,at,inode,offset FROM session_records WHERE path=?1 AND kind='tool_call' AND rowid>?2 AND rowid<?3 ORDER BY rowid LIMIT ?4")
            .map_err(err)?
            .query_map(params![path, start, until, limit as i64 + 1], row)
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

    /// One call by its id, wherever in the conversation it is.
    pub fn tool_call(&self, path: &str, id: &str) -> io::Result<Option<ToolRow>> {
        let Some(mut found) = self
            .db
            .prepare_cached("SELECT rowid,call_id,text,at,inode,offset FROM session_records WHERE path=?1 AND call_id=?2 AND kind='tool_call' ORDER BY rowid LIMIT 1")
            .map_err(err)?
            .query_row(params![path, id], row)
            .optional()
            .map_err(err)?
        else {
            return Ok(None);
        };
        found.results = self
            .db
            .prepare_cached(&format!("{RESULT} FROM session_records WHERE path=?1 AND call_id=?2 AND kind='tool_result' ORDER BY rowid LIMIT {MOST_RESULTS}"))
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
        let wanted: HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        let mut found: HashMap<String, Vec<ToolResultRow>> = HashMap::new();
        let mut q = self
            .db
            .prepare_cached("SELECT rowid,at,exit_code,inode,offset,call_id FROM session_records WHERE path=?1 AND kind='tool_result' AND rowid>?2 ORDER BY rowid")
            .map_err(err)?;
        let mut hits = q.query(params![path, first]).map_err(err)?;
        while let Some(r) = hits.next().map_err(err)? {
            let id: String = r.get(5).map_err(err)?;
            if wanted.contains(id.as_str()) {
                let results = found.entry(id).or_default();
                if results.len() < MOST_RESULTS {
                    results.push(result(r).map_err(err)?);
                }
            }
        }
        for r in rows {
            r.results = found.remove(&r.id).unwrap_or_default();
        }
        Ok(())
    }

    /// What a whole stretch adds up to, as `chat`'s `tools` counts it.
    fn stretch(&self, path: &str, from: i64, until: i64) -> io::Result<ChatTools> {
        let failed = self
            .db
            .prepare_cached("SELECT call_id FROM session_records WHERE path=?1 AND kind='tool_result' AND exit_code<>0 AND rowid>?2")
            .map_err(err)?
            .query_map(params![path, from], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(err)?;
        let calls = self
            .db
            .prepare_cached("SELECT call_id,text FROM session_records WHERE path=?1 AND kind='tool_call' AND rowid>?2 AND rowid<?3 ORDER BY rowid")
            .map_err(err)?
            .query_map(params![path, from, until], |r| {
                let id: String = r.get(0)?;
                Ok((r.get::<_, String>(1)?, failed.contains(&id)))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(summary(&calls))
    }
}
