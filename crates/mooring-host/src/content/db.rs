//! Durable content: session cards, subagent links and saved parser state in SQLite.
use super::*;

pub(super) fn db_err(e: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::other(e)
}
pub fn schema(db: &Connection) -> io::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS session_content(path TEXT PRIMARY KEY,title TEXT NOT NULL,turn TEXT NOT NULL,updated INTEGER NOT NULL,projection TEXT NOT NULL,parser TEXT NOT NULL); CREATE TABLE IF NOT EXISTS subagent_links(path TEXT PRIMARY KEY,parent TEXT NOT NULL,name TEXT NOT NULL,turn TEXT NOT NULL,waiting TEXT NOT NULL,updated INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS subagent_parent ON subagent_links(parent); CREATE TABLE IF NOT EXISTS session_records(path TEXT NOT NULL,inode INTEGER NOT NULL,offset INTEGER NOT NULL,ordinal INTEGER NOT NULL,at INTEGER NOT NULL,kind TEXT NOT NULL,text TEXT NOT NULL,call_id TEXT NOT NULL,exit_code INTEGER,PRIMARY KEY(path,inode,offset,ordinal)); CREATE TABLE IF NOT EXISTS log_backfill(path TEXT PRIMARY KEY,inode INTEGER NOT NULL,origin INTEGER NOT NULL,start INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS session_records_messages ON session_records(path,inode,offset,ordinal) WHERE kind IN ('user','assistant');
        CREATE INDEX IF NOT EXISTS session_records_call ON session_records(path,call_id,kind,offset,ordinal);
        CREATE INDEX IF NOT EXISTS session_records_turn_start ON session_records(path,at) WHERE kind='turn' AND text='Running';
        CREATE INDEX IF NOT EXISTS session_records_tools ON session_records(path,offset,ordinal) WHERE kind='tool_call';
        CREATE INDEX IF NOT EXISTS session_records_stops ON session_records(path,inode,offset,ordinal) WHERE kind='turn' AND text='Interrupted';").map_err(db_err)
}
pub fn load(db: &Connection, path: &str) -> io::Result<Parser> {
    let value: Option<String> = db
        .prepare_cached("SELECT parser FROM session_content WHERE path=?1")
        .map_err(db_err)?
        .query_row([path], |r| r.get(0))
        .optional()
        .map_err(db_err)?;
    value
        .map(|s| serde_json::from_str(&s).map_err(db_err))
        .unwrap_or_else(|| Ok(Parser::default()))
}
pub fn card(db: &Connection, path: &str) -> io::Result<Option<Content>> {
    let value: Option<String> = db
        .prepare_cached("SELECT projection FROM session_content WHERE path=?1")
        .map_err(db_err)?
        .query_row([path], |r| r.get(0))
        .optional()
        .map_err(db_err)?;
    value
        .map(|s| serde_json::from_str(&s).map_err(db_err))
        .transpose()
}
/// With `MOORING_STATUS_TRACE=<file>`, appends one line per turn change for regression
/// checks against a transcript. Paths and states only; no prompt or reply text.
pub(super) fn trace(path: &str, parser: &Parser) {
    let Some(file) = std::env::var_os("MOORING_STATUS_TRACE") else {
        return;
    };
    if parser.transitions.is_empty() {
        return;
    }
    let lines: String = parser
        .transitions
        .iter()
        .map(|t| format!("{} {path} {t}\n", mooring_core::content::now()))
        .collect();
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut f| std::io::Write::write_all(&mut f, lines.as_bytes()));
}
pub fn save(db: &Connection, path: &str, parser: &Parser) -> io::Result<()> {
    save_state(db, path, parser)?;
    save_records(db, path, parser)
}
/// The card and the parser state, which the next line continues from.
pub fn save_state(db: &Connection, path: &str, parser: &Parser) -> io::Result<()> {
    db.prepare_cached("INSERT INTO session_content VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(path) DO UPDATE SET title=excluded.title,turn=excluded.turn,updated=excluded.updated,projection=excluded.projection,parser=excluded.parser").map_err(db_err)?.execute(params![path,parser.card.title,format!("{:?}",parser.card.turn),i64::try_from(parser.card.updated).map_err(db_err)?,serde_json::to_string(&parser.card).map_err(db_err)?,serde_json::to_string(parser).map_err(db_err)?]).map_err(db_err)?;
    Ok(())
}
/// The records and status transitions this read produced; keyed by where each was read,
/// so writing them again after a replay changes nothing.
pub fn save_records(db: &Connection, path: &str, parser: &Parser) -> io::Result<()> {
    trace(path, parser);
    let mut ordinals = BTreeMap::new();
    for r in &parser.records {
        let i = ordinals
            .entry((r.source.inode, r.source.offset))
            .or_insert(0u64);
        let ordinal = *i;
        *i += 1;
        db.prepare_cached(
            "INSERT OR IGNORE INTO session_records VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        )
        .map_err(db_err)?
        .execute(params![
            path,
            i64::try_from(r.source.inode).map_err(db_err)?,
            i64::try_from(r.source.offset).map_err(db_err)?,
            ordinal as i64,
            i64::try_from(r.at).map_err(db_err)?,
            r.kind,
            r.text,
            r.call_id,
            r.code
        ])
        .map_err(db_err)?;
    }
    Ok(())
}
/// A tool call's arguments as its `tool_call` record keeps them: whole up to 8 KiB, else
/// only the fields read later (`kept_input`: paths, the command, line counts), so a long
/// Write still says which file it wrote and how many lines (the phone's `chat` `tools`).
/// A long Codex `exec` keeps the files its script patches, read while the script is whole.
pub(super) fn logged_args(name: &str, args: &Value) -> String {
    let whole = args.to_string();
    if whole.chars().count() <= 8192 {
        return whole;
    }
    let mut kept = kept_input(args);
    if let (true, Some(script)) = (name == "exec", args["input"].as_str()) {
        kept[PATCHED] = json!(crate::agents::codex::script_patches(script));
    }
    kept.to_string()
}
