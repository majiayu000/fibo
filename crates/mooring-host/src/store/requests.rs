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

/// Stands in the error column while a named act runs: found later, the host stopped in
/// the middle and cannot tell whether the act happened.
const STARTED: &str = "\u{0}started";
pub const UNSURE: &str =
    "电脑上的 Fibo 在执行这一步时停了，不确定执行了没有；看一眼屏幕再决定要不要重发";

impl Store {
    /// The outcome recorded for `request`: applied (`Ok`) or refused with its error; one
    /// that started and never finished is reported as [`UNSURE`], never run again.
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
        Ok(error.map(|e| match e {
            None => Ok(()),
            Some(e) if e == STARTED => Err(UNSURE.to_owned()),
            Some(e) => Err(e),
        }))
    }
    /// Marks a named act as begun, before it touches anything (typing into a terminal).
    pub fn started(&mut self, request: &RequestId) -> io::Result<()> {
        record(&self.0, request, Some(STARTED))
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
        "INSERT OR REPLACE INTO requests VALUES(?1,?2,?3,?4)",
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
        // Begun and never finished: unsure, so a resend does not act again.
        store.started(&id("typing")).unwrap();
        assert_eq!(
            store.settled(&id("typing")).unwrap(),
            Some(Err(UNSURE.into()))
        );
        store.applied(&id("typing")).unwrap();
        assert_eq!(store.settled(&id("typing")).unwrap(), Some(Ok(())));
        std::fs::remove_dir_all(root).unwrap();
    }
}
