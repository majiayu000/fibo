//! Direct child-thread projections, shared by every adapter that names its parent.
use super::db::db_err;
use super::*;

/// What a parent's subagents are doing, from `subagent_links`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Children {
    pub running: u32,
    pub waiting: Option<String>,
    pub subagents: Vec<mooring_core::content::Subagent>,
}
/// A subagent whose log has not moved for this long no longer counts as running: its
/// process may have died mid-turn.
pub(super) const CHILD_FRESH: u64 = 900;
pub fn link_child(db: &Connection, path: &str, card: &Content) -> io::Result<()> {
    db.prepare_cached("INSERT INTO subagent_links VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(path) DO UPDATE SET parent=excluded.parent,name=excluded.name,turn=excluded.turn,waiting=excluded.waiting,updated=excluded.updated")
        .map_err(db_err)?
        .execute(params![
            path,
            card.parent,
            card.agent_name,
            format!("{:?}", card.turn),
            card.waiting,
            i64::try_from(card.updated).map_err(db_err)?
        ])
        .map_err(db_err)?;
    Ok(())
}
pub fn children(
    db: &Connection,
    runtime: Agent,
    parent: &str,
    now: u64,
    turn_started: u64,
) -> io::Result<Children> {
    // Reuse the indexed turn receipts: no second timeline or transcript scan.
    // Completed children remain visible for their parent's entire turn, even after 15 min.
    let mut q = db.prepare_cached(
        "SELECT l.name,l.turn,l.waiting,l.updated,f.cli_id,
            (SELECT MAX(at) FROM session_records WHERE path=l.path AND kind='turn' AND text='Running')
         FROM subagent_links l JOIN log_files f ON f.path=l.path
         WHERE l.parent=?1 AND l.updated>=?2 AND f.runtime=?3 ORDER BY l.updated DESC,l.path"
    ).map_err(db_err)?;
    let fresh = now.saturating_sub(CHILD_FRESH);
    let since = i64::try_from(turn_started.min(fresh)).map_err(db_err)?;
    let mut rows = q
        .query(params![parent, since, runtime.executable()])
        .map_err(db_err)?;
    let mut kids = Children::default();
    while let Some(r) = rows.next().map_err(db_err)? {
        let name: String = r.get(0).map_err(db_err)?;
        let turn: String = r.get(1).map_err(db_err)?;
        let updated = u64::try_from(r.get::<_, i64>(3).map_err(db_err)?).map_err(db_err)?;
        let running = matches!(turn.as_str(), "Running" | "Waiting") && updated >= fresh;
        if running {
            kids.running += 1;
            if turn == "Waiting" && kids.waiting.is_none() {
                let waiting: String = r.get(2).map_err(db_err)?;
                let label = if name.is_empty() {
                    "子 Agent".into()
                } else {
                    format!("子 Agent {name}")
                };
                kids.waiting = Some(format!("{label}：{}", short(&waiting, 500)));
            }
        }
        let id: String = r.get(4).map_err(db_err)?;
        let started = r
            .get::<_, Option<i64>>(5)
            .map_err(db_err)?
            .map(u64::try_from)
            .transpose()
            .map_err(db_err)?
            .unwrap_or(updated);
        kids.subagents.push(mooring_core::content::Subagent {
            name: if name.is_empty() { id.clone() } else { name },
            id,
            started,
            ended: (!running).then_some(updated.max(started)),
            stopped: !running && turn != "Completed",
            foreground: false,
        });
    }
    kids.subagents
        .sort_by(|a, b| (a.started, &a.id).cmp(&(b.started, &b.id)));
    Ok(kids)
}
