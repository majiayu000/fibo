//! What the host started each terminal's Agent with (`Workspace::agent_starts`), in a
//! table of its own: a host from before it neither reads nor writes it, and a database such
//! a host wrote simply has none, so its terminals keep the evidence they always had.
use super::db_error;
use fibo_core::model::{AgentStart, Workspace};
use rusqlite::{params, Connection};
use std::io;

pub(super) const TABLE: &str = "CREATE TABLE IF NOT EXISTS agent_starts (terminal_id TEXT PRIMARY KEY REFERENCES terminals(id), start TEXT NOT NULL);";

/// Every start this host can read. One it cannot is no evidence at all, never a failed
/// load: the row stays as it was, and nothing here writes it back.
pub(super) fn load(db: &Connection, w: &mut Workspace) -> io::Result<()> {
    let mut statement = db
        .prepare("SELECT terminal_id,start FROM agent_starts")
        .map_err(db_error)?;
    let rows = statement
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(db_error)?;
    for row in rows {
        let (terminal, start) = row.map_err(db_error)?;
        if let Ok(start) = serde_json::from_str::<AgentStart>(&start) {
            w.agent_starts.insert(terminal, start);
        }
    }
    Ok(())
}

pub(super) fn save(db: &Connection, before: &Workspace, w: &Workspace) -> io::Result<()> {
    for id in before.agent_starts.keys() {
        if !w.agent_starts.contains_key(id) {
            db.execute("DELETE FROM agent_starts WHERE terminal_id=?1", [id])
                .map_err(db_error)?;
        }
    }
    for (id, start) in &w.agent_starts {
        if before.agent_starts.get(id) != Some(start) {
            db.execute(
                "INSERT INTO agent_starts VALUES(?1,?2) ON CONFLICT(terminal_id) DO UPDATE SET start=excluded.start",
                params![id, serde_json::to_string(start).map_err(db_error)?],
            )
            .map_err(db_error)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn a_start_reads_back_and_a_database_from_before_loads_without_any() {
        let dir = std::env::temp_dir().join(format!("kl-starts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let kept: std::collections::HashSet<String> = ["t".to_owned()].into();
        let (mut db, before) = Store::open_keeping(&dir, &kept).unwrap();
        let mut after = before.clone();
        after.spaces.push(Space {
            id: "s".into(),
            name: "s".into(),
            cwd: "/tmp".into(),
        });
        after.terminals.push(Terminal {
            id: "t".into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into()],
            cwd: "/tmp".into(),
            pid: Some(1),
            state: ProcessState::Running,
            exit_code: None,
            error: None,
        });
        let start = AgentStart {
            at: 5,
            ..AgentStart::new(Agent::Codex, Some("两行\n`任务`".into()))
        };
        after.agent_starts.insert("t".into(), start.clone());
        db.save(&before, &after).unwrap();
        drop(db);
        let (db, loaded) = Store::open_keeping(&dir, &kept).unwrap();
        assert_eq!(loaded.agent_starts.get("t"), Some(&start));
        // A row this host cannot read is no evidence, never a failed load.
        db.0.execute("UPDATE agent_starts SET start='{\"agent\":1}'", [])
            .unwrap();
        drop(db);
        let (db, unreadable) = Store::open_keeping(&dir, &kept).unwrap();
        assert!(unreadable.agent_starts.is_empty());
        assert_eq!(unreadable.terminals.len(), 1);
        // What a host from before wrote: no such table at all.
        db.0.execute_batch("DROP TABLE agent_starts").unwrap();
        drop(db);
        let (_, old) = Store::open_keeping(&dir, &kept).unwrap();
        assert!(old.agent_starts.is_empty());
        assert_eq!(old.terminals[0].state, ProcessState::Running);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
