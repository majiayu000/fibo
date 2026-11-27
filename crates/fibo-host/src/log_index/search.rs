//! The desktop palette searches saved messages independently of the session cards.
use super::*;
use fibo_core::protocol::{MessageMatch, MessagePosition};

const LIMIT: usize = 100;

impl Index {
    pub fn search_messages(
        &self,
        query: &str,
        sessions: &[String],
        before: Option<&MessagePosition>,
    ) -> io::Result<(Vec<MessageMatch>, Option<MessagePosition>)> {
        search(&self.db, query, sessions, before)
    }

    pub fn read_search_message(
        &self,
        position: &MessagePosition,
        generation: &str,
    ) -> io::Result<String> {
        read_message(&self.db, position, generation)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "saved message no longer exists")
        })
    }
}

fn search(
    db: &Connection,
    query: &str,
    sessions: &[String],
    before: Option<&MessagePosition>,
) -> io::Result<(Vec<MessageMatch>, Option<MessagePosition>)> {
    let query = query.trim();
    if query.is_empty() || sessions.is_empty() {
        return Ok((Vec::new(), None));
    }
    let sessions = serde_json::to_string(sessions).map_err(err)?;
    // Match the whole saved message, then only send the passage around the match.
    // A strict bound on the full sort key keeps equal timestamps and new arrivals
    // from repeating or skipping rows on the following page.
    let mut statement = db
        .prepare_cached(
            "WITH matches AS (
                SELECT s.id, r.path, r.at, r.kind, r.text, r.inode, r.offset, r.ordinal,
                       instr(lower(r.text), lower(?2)) AS position
                FROM agent_sessions s JOIN session_records r ON r.path = s.transcript
                WHERE s.id IN (SELECT value FROM json_each(?1))
                  AND r.kind IN ('user','assistant')
                  AND (?4 IS NULL OR (r.at, s.id, r.inode, r.offset, r.ordinal) < (?4, ?5, ?6, ?7, ?8))
                  AND position > 0
                ORDER BY r.at DESC, s.id DESC, r.inode DESC, r.offset DESC, r.ordinal DESC
                LIMIT ?3
            )
            SELECT id, at, inode, offset, ordinal, kind = 'user', coalesce(g.generation, ''),
                   CASE WHEN position > 61 THEN '…' ELSE '' END ||
                       substr(text, max(1, position - 60), max(280, length(?2) + 120)) ||
                       CASE WHEN length(text) >= max(1, position - 60) + max(280, length(?2) + 120)
                            THEN '…' ELSE '' END
            FROM matches LEFT JOIN log_generations g ON g.path = matches.path
            ORDER BY at DESC, id DESC, inode DESC, offset DESC, ordinal DESC",
        )
        .map_err(err)?;
    let mut messages = statement
        .query_map(
            params![
                sessions,
                query,
                LIMIT as i64 + 1,
                before.map(|p| p.at),
                before.map(|p| p.session.as_str()),
                before.map(|p| p.inode),
                before.map(|p| p.offset),
                before.map(|p| p.ordinal)
            ],
            |row| {
                Ok(MessageMatch {
                    position: MessagePosition {
                        session: row.get(0)?,
                        at: row.get(1)?,
                        inode: row.get(2)?,
                        offset: row.get(3)?,
                        ordinal: row.get(4)?,
                    },
                    user: row.get(5)?,
                    generation: row.get(6)?,
                    text: row.get(7)?,
                })
            },
        )
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    let next = if messages.len() > LIMIT {
        messages.truncate(LIMIT);
        messages.last().map(|hit| hit.position.clone())
    } else {
        None
    };
    Ok((messages, next))
}

fn read_message(
    db: &Connection,
    position: &MessagePosition,
    generation: &str,
) -> io::Result<Option<String>> {
    db.prepare_cached(
        "SELECT r.text
         FROM agent_sessions s JOIN session_records r ON r.path = s.transcript
         LEFT JOIN log_generations g ON g.path = r.path
         WHERE s.id = ?1 AND r.inode = ?2 AND r.offset = ?3 AND r.ordinal = ?4 AND r.at = ?5
           AND r.kind IN ('user','assistant') AND coalesce(g.generation, '') = ?6",
    )
    .map_err(err)?
    .query_row(
        params![
            position.session,
            position.inode,
            position.offset,
            position.ordinal,
            position.at,
            generation
        ],
        |row| row.get(0),
    )
    .optional()
    .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::content::schema(&db).unwrap();
        db.execute_batch(
            "CREATE TABLE agent_sessions(id TEXT PRIMARY KEY, transcript TEXT NOT NULL);
             CREATE TABLE log_generations(path TEXT PRIMARY KEY, generation TEXT NOT NULL);
             INSERT INTO agent_sessions VALUES ('s', 'log'), ('hidden', 'other');
             INSERT INTO log_generations VALUES ('log', 'original');",
        )
        .unwrap();
        db
    }

    #[test]
    fn search_reads_old_prompts_and_replies_in_visible_sessions() {
        let db = fixture();
        db.execute_batch(
            "INSERT INTO session_records VALUES ('log',1,10,0,10,'user','昨天 #636 是什么意思','',NULL);
             INSERT INTO session_records VALUES ('log',1,20,0,20,'assistant','海鹏，有两个画布的 PR #636','',NULL);
             INSERT INTO session_records VALUES ('log',1,30,0,30,'tool_result','#636','',NULL);
             INSERT INTO session_records VALUES ('other',1,30,0,30,'user','#636','',NULL);
             INSERT INTO session_records VALUES ('log',1,40,0,40,'user','今天 #636 有评论了','',NULL);",
        )
        .unwrap();

        let (hits, next) = search(&db, "#636", &["s".into()], None).unwrap();
        assert!(next.is_none());
        assert_eq!(
            hits.iter()
                .map(|hit| (hit.position.at, hit.user))
                .collect::<Vec<_>>(),
            [(40, true), (20, false), (10, true)]
        );
        let (hits, _) = search(&db, "画布的 pr", &["s".into()], None).unwrap();
        assert_eq!(hits[0].text, "海鹏，有两个画布的 PR #636");
        let (hits, _) = search(&db, "是什么意思", &["s".into()], None).unwrap();
        assert_eq!(hits[0].text, "昨天 #636 是什么意思");
    }

    #[test]
    fn pages_cross_equal_timestamps_with_no_gaps_or_duplicates() {
        let db = fixture();
        db.execute_batch(
            "WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<207)
             INSERT INTO session_records
             SELECT CASE WHEN n % 2 = 0 THEN 'log' ELSE 'other' END, 1, n/4, n%4, 100,
                    'user', 'match', '', NULL FROM numbers;",
        )
        .unwrap();
        let sessions = ["s".into(), "hidden".into()];
        let (first, cursor) = search(&db, "match", &sessions, None).unwrap();
        db.execute(
            "INSERT INTO session_records VALUES ('log',1,200,0,101,'user','match','',NULL)",
            [],
        )
        .unwrap();
        let (second, cursor) = search(&db, "match", &sessions, cursor.as_ref()).unwrap();
        let (last, cursor) = search(&db, "match", &sessions, cursor.as_ref()).unwrap();
        assert_eq!((first.len(), second.len(), last.len()), (100, 100, 7));
        assert!(cursor.is_none());
        let ids = first
            .iter()
            .chain(&second)
            .chain(&last)
            .map(|m| m.position.offset * 4 + m.position.ordinal)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            ids.into_iter().collect::<Vec<_>>(),
            (1..=207).collect::<Vec<_>>()
        );
    }

    #[test]
    fn opens_the_exact_message_instead_of_an_excerpt_or_nearby_match() {
        let db = fixture();
        let text = format!("{} #636 {}", "opening ".repeat(60), "closing ".repeat(60));
        db.execute(
            "INSERT INTO session_records VALUES ('log',1,10,0,10,'assistant',?1,'',NULL)",
            [&text],
        )
        .unwrap();
        db.execute("INSERT INTO session_records VALUES ('log',1,10,1,10,'user','#636 different message','',NULL)", []).unwrap();
        let (hits, _) = search(&db, "#636", &["s".into()], None).unwrap();
        let hit = &hits[1];
        assert_ne!(hit.text, text);
        assert_eq!(
            read_message(&db, &hit.position, &hit.generation).unwrap(),
            Some(text)
        );
        db.execute(
            "UPDATE log_generations SET generation='rewritten' WHERE path='log'",
            [],
        )
        .unwrap();
        assert!(read_message(&db, &hit.position, &hit.generation)
            .unwrap()
            .is_none());
    }
}
