//! Reconcile a confirmed deletion against the cursor, content and live projection.
use super::*;

impl Index {
    /// An unavailable log root cannot tell us which of its transcripts were removed.
    pub(super) fn can_confirm_absence(&self, path: &Path) -> io::Result<bool> {
        let root = self
            .roots
            .iter()
            .find_map(|(_, root)| path.starts_with(root).then_some(root.as_path()))
            .or_else(|| path.parent());
        let Some(root) = root else { return Ok(false) };
        match fs::symlink_metadata(root) {
            Ok(meta) => Ok(meta.is_dir()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(super) fn delete_log(&mut self, held: &mut Held, path: &str) -> io::Result<()> {
        let tx = self.db.savepoint().map_err(err)?;
        let parent: Option<(String, String)> = tx
            .prepare_cached(
                "SELECT f.runtime,l.parent FROM subagent_links l
                 JOIN log_files f ON f.path=l.path WHERE l.path=?1",
            )
            .map_err(err)?
            .query_row([path], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(err)?;
        let mut removed = 0;
        for sql in [
            "DELETE FROM log_files WHERE path=?1",
            "DELETE FROM session_content WHERE path=?1",
            "DELETE FROM session_records WHERE path=?1",
            "DELETE FROM subagent_links WHERE path=?1",
            "DELETE FROM log_backfill WHERE path=?1",
            "DELETE FROM log_generations WHERE path=?1",
        ] {
            removed += tx
                .prepare_cached(sql)
                .map_err(err)?
                .execute([path])
                .map_err(err)?;
        }
        let touched = if let Some((runtime, parent)) = parent {
            refresh_parent(&tx, held, agent(&runtime)?, &parent, path, true)?
        } else {
            None
        };
        tx.commit().map_err(err)?;
        let was_held = held.entries.remove(path).is_some();
        if removed > 0 || was_held {
            held.removed.push(path.to_owned());
        }
        self.touched.retain(|t| t.path != path);
        self.touched.extend(touched.map(|p| p.publish(held)));
        self.counts.writes += u64::from(removed > 0);
        Ok(())
    }

    /// Clear the corresponding workspace cards after the index transaction committed.
    /// A failed workspace update keeps the notification for the retry.
    pub fn publish_removals(
        &self,
        publish: impl FnOnce(&[String]) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut held = self
            .gate
            .lock()
            .map_err(|_| io::Error::other("log cursor lock poisoned"))?;
        let removed = std::mem::take(&mut held.removed);
        drop(held);
        if removed.is_empty() {
            return Ok(());
        }
        if let Err(error) = publish(&removed) {
            let mut held = self
                .gate
                .lock()
                .map_err(|_| io::Error::other("log cursor lock poisoned"))?;
            held.removed.extend(removed);
            return Err(error);
        }
        Ok(())
    }
}

pub(super) struct ParentRefresh {
    transcript: Transcript,
    parser: Option<crate::content::Parser>,
}

impl ParentRefresh {
    pub(super) fn publish(self, held: &mut Held) -> Transcript {
        if let (Some(parser), Some(e)) = (self.parser, held.entries.get_mut(&self.transcript.path))
        {
            e.parser = parser;
            e.dirty = None;
        }
        self.transcript
    }
}

/// Rebuild the one parent projection affected by a child log change.
pub(super) fn refresh_parent(
    db: &Connection,
    held: &mut Held,
    runtime: Agent,
    parent: &str,
    child_path: &str,
    persist: bool,
) -> io::Result<Option<ParentRefresh>> {
    let row = db
        .prepare_cached(
            "SELECT cli_id,path,cwd,updated FROM log_files WHERE runtime=?2 AND cli_id=?1 LIMIT 1",
        )
        .map_err(err)?
        .query_row([parent, runtime.executable()], |r| {
            Ok(Transcript {
                runtime,
                cli_id: r.get(0)?,
                path: r.get(1)?,
                cwd: r.get(2)?,
                updated: r.get::<_, i64>(3)? as u64,
            })
        })
        .optional()
        .map_err(err)?;
    let Some(t) = row.filter(|t| t.path != child_path) else {
        return Ok(None);
    };
    let now = fibo_core::content::now();
    let source = fibo_core::content::Source {
        path: t.path.clone(),
        offset: now,
        ..Default::default()
    };
    let parser = if let Some(e) = held.entries.get_mut(&t.path) {
        let kids = crate::content::children(db, runtime, parent, now, e.parser.card.turn_started)?;
        if persist {
            let mut updated = e.parser.clone();
            updated.children(&kids, now, source);
            write_cursor(db, &t.path, e.runtime, &e.cursor)?;
            crate::content::save_state(db, &t.path, &updated)?;
            Some(updated)
        } else {
            e.parser.children(&kids, now, source);
            e.dirty.get_or_insert_with(std::time::Instant::now);
            None
        }
    } else {
        let mut owner = crate::content::load(db, &t.path)?;
        let kids = crate::content::children(db, runtime, parent, now, owner.card.turn_started)?;
        owner.children(&kids, now, source);
        crate::content::save(db, &t.path, &owner)?;
        None
    };
    Ok(Some(ParentRefresh {
        transcript: t,
        parser,
    }))
}
