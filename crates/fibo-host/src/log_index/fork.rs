//! A second connection to the host's index, set up in two steps so the host lock covers
//! only the cheap one: [`Index::forker`] copies what the fork needs, [`Fork::open`] opens
//! the database after the lock is let go.
use super::*;

/// What a fork of the index needs, taken under the host lock.
pub struct Fork {
    db: PathBuf,
    roots: Vec<(Agent, PathBuf)>,
    recent_days: u64,
    gate: std::sync::Arc<std::sync::Mutex<Held>>,
}

impl Index {
    pub fn forker(&self) -> io::Result<Fork> {
        let db = self
            .db
            .path()
            .ok_or_else(|| io::Error::other("index database path unavailable"))?;
        Ok(Fork {
            db: db.into(),
            roots: self.roots.clone(),
            recent_days: self.recent_days,
            gate: self.gate.clone(),
        })
    }
}

impl Fork {
    /// Opens the connection. The tables are the host's own index's, made when it opened,
    /// so none of the ~20 schema statements run again (about 1.1 ms a fork on a real index,
    /// against 0.008 ms reusing a connection).
    pub fn open(self) -> io::Result<Index> {
        let db = Connection::open(&self.db).map_err(err)?;
        db.busy_timeout(Duration::from_secs(5)).map_err(err)?;
        // Per connection, as the host's own: WAL itself is kept in the database file.
        db.execute_batch("PRAGMA synchronous=NORMAL;")
            .map_err(err)?;
        Ok(Index {
            db,
            roots: self.roots,
            recent_days: self.recent_days,
            counts: IoCounts::default(),
            gate: self.gate,
            touched: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fork reads what the host's index wrote and shares its gate, with no schema run.
    #[test]
    fn a_fork_reads_the_same_index_and_shares_its_gate() {
        let root = std::env::temp_dir().join(format!("kl-fork-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let index = Index::open(&root, vec![(Agent::Codex, root.join("logs"))]).unwrap();
        index
            .db
            .execute(
                "INSERT INTO log_scan_dirs(path,runtime) VALUES ('/probe','codex')",
                [],
            )
            .unwrap();
        let fork = index.forker().unwrap().open().unwrap();
        let rows: i64 = fork
            .db
            .query_row("SELECT count(*) FROM log_scan_dirs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        assert!(std::sync::Arc::ptr_eq(&fork.gate, &index.gate));
        assert_eq!(
            (fork.roots.clone(), fork.recent_days),
            (index.roots.clone(), index.recent_days)
        );
        drop((fork, index));
        std::fs::remove_dir_all(root).unwrap();
    }
}
