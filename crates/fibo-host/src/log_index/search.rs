//! The desktop palette searches saved messages independently of the session cards.
use super::*;
use fibo_core::protocol::MessageMatch;

const LIMIT: usize = 100;

impl Index {
    pub fn search_messages(
        &self,
        query: &str,
        sessions: &[String],
    ) -> io::Result<(Vec<MessageMatch>, bool)> {
        search(&self.db, query, sessions)
    }
}

fn search(
    db: &Connection,
    query: &str,
    sessions: &[String],
) -> io::Result<(Vec<MessageMatch>, bool)> {
    let query = query.trim();
    if query.is_empty() || sessions.is_empty() {
        return Ok((Vec::new(), false));
    }
    let sessions = serde_json::to_string(sessions).map_err(err)?;
    // Match the whole saved message, then only send the passage around the match.
    // The partial messages index keeps tool output out of the search.
    let mut statement = db
        .prepare_cached(
            "WITH matches AS (
                SELECT s.id, r.at, r.kind, r.text, r.offset, r.ordinal,
                       instr(lower(r.text), lower(?2)) AS position
                FROM agent_sessions s JOIN session_records r ON r.path = s.transcript
                WHERE s.id IN (SELECT value FROM json_each(?1))
                  AND r.kind IN ('user','assistant') AND position > 0
                ORDER BY r.at DESC, s.id, r.offset DESC, r.ordinal DESC
                LIMIT ?3
            )
            SELECT id, at, kind = 'user',
                   CASE WHEN position > 61 THEN '…' ELSE '' END ||
                       substr(text, max(1, position - 60), max(280, length(?2) + 120)) ||
                       CASE WHEN length(text) >= max(1, position - 60) + max(280, length(?2) + 120)
                            THEN '…' ELSE '' END
            FROM matches
            ORDER BY at DESC, id, offset DESC, ordinal DESC",
        )
        .map_err(err)?;
    let mut messages = statement
        .query_map(params![sessions, query, LIMIT as i64 + 1], |row| {
            Ok(MessageMatch {
                session: row.get(0)?,
                at: row.get::<_, i64>(1)?.max(0) as u64,
                user: row.get(2)?,
                text: row.get(3)?,
            })
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    let more = messages.len() > LIMIT;
    messages.truncate(LIMIT);
    Ok((messages, more))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_reads_old_prompts_and_replies_in_visible_sessions() {
        let db = Connection::open_in_memory().unwrap();
        crate::content::schema(&db).unwrap();
        db.execute_batch(
            "CREATE TABLE agent_sessions(id TEXT PRIMARY KEY, transcript TEXT NOT NULL);
             INSERT INTO agent_sessions VALUES ('s', 'log'), ('hidden', 'other');
             INSERT INTO session_records VALUES ('log',1,10,0,10,'user','昨天 #636 是什么意思','',NULL);
             INSERT INTO session_records VALUES ('log',1,20,0,20,'assistant','海鹏，有两个画布的 PR #636','',NULL);
             INSERT INTO session_records VALUES ('log',1,30,0,30,'tool_result','#636','',NULL);
             INSERT INTO session_records VALUES ('other',1,30,0,30,'user','#636','',NULL);
             INSERT INTO session_records VALUES ('log',1,40,0,40,'user','今天 #636 有评论了','',NULL);",
        )
        .unwrap();

        let (hits, more) = search(&db, "#636", &["s".into()]).unwrap();
        assert!(!more);
        assert_eq!(
            hits.iter()
                .map(|hit| (hit.at, hit.user))
                .collect::<Vec<_>>(),
            [(40, true), (20, false), (10, true)]
        );
        let (hits, _) = search(&db, "画布的 pr", &["s".into()]).unwrap();
        assert_eq!(hits[0].text, "海鹏，有两个画布的 PR #636");
        let (hits, _) = search(&db, "是什么意思", &["s".into()]).unwrap();
        assert_eq!(hits[0].text, "昨天 #636 是什么意思");
    }
}
