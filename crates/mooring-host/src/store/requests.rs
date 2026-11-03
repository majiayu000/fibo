//! What became of each named write (`RequestId`), so a resend takes effect once. An
//! applied write is recorded in the transaction that saves its change, so the change and
//! its record land together or not at all; a refused one is recorded on its own.
use super::{db_error, Store};
use mooring_core::protocol::{RequestId, REQUEST_KEPT_SECS};
use rusqlite::{params, Connection, OptionalExtension};
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const TABLE: &str = "CREATE TABLE IF NOT EXISTS requests (client TEXT NOT NULL, id TEXT NOT NULL, at INTEGER NOT NULL, error TEXT, PRIMARY KEY(client,id));
    CREATE INDEX IF NOT EXISTS requests_at ON requests(at);";

impl Store {
    /// The outcome recorded for `request`: applied (`Ok`) or refused with its error.
    pub fn settled(&self, request: &RequestId) -> io::Result<Option<Result<(), String>>> {
        let error = self
            .0
            .query_row(
                "SELECT error FROM requests WHERE client=?1 AND id=?2",
                params![request.client, request.id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(db_error)?;
        Ok(error.map(|e| e.map_or(Ok(()), Err)))
    }
    /// Records a request that changed nothing stored here (typing into a terminal).
    pub fn applied(&mut self, request: &RequestId) -> io::Result<()> {
        record(&self.0, request, None)
    }
    pub fn refused(&mut self, request: &RequestId, error: &str) -> io::Result<()> {
        record(&self.0, request, Some(error))
    }
}

/// Records an outcome and forgets those older than a day.
pub(super) fn record(db: &Connection, request: &RequestId, error: Option<&str>) -> io::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let now = i64::try_from(now).map_err(db_error)?;
    let kept = i64::try_from(REQUEST_KEPT_SECS).map_err(db_error)?;
    db.execute("DELETE FROM requests WHERE at < ?1", [now - kept])
        .map_err(db_error)?;
    db.execute(
        "INSERT INTO requests VALUES(?1,?2,?3,?4)",
        params![request.client, request.id, now, error],
    )
    .map_err(db_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes_are_kept_a_day_and_then_forgotten() {
        let root = std::env::temp_dir().join(format!("kl-rq-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let (mut store, _) = Store::open(&root).unwrap();
        let id = |n: &str| RequestId {
            client: "c".into(),
            id: n.into(),
        };
        store
            .0
            .execute("INSERT INTO requests VALUES('c','old',0,NULL)", [])
            .unwrap();
        assert_eq!(store.settled(&id("old")).unwrap(), Some(Ok(())));
        store.refused(&id("new"), "不行").unwrap();
        assert_eq!(store.settled(&id("old")).unwrap(), None);
        assert_eq!(store.settled(&id("new")).unwrap(), Some(Err("不行".into())));
        assert_eq!(store.settled(&id("other")).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
