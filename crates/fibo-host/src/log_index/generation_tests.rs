//! A chat's generation changes when its log is rewritten, so the phone drops its cursor;
//! an index from an older build (no generation table) reads back without a backfill.
use super::*;

#[test]
fn truncation_and_inode_replacement_start_a_new_generation_and_old_indexes_need_no_backfill() {
    let f = Fixture::new();
    let path = f.0.join("logs/one.jsonl");
    fs::write(&path, line("one")).unwrap();
    let mut i = f.index();
    i.update(Agent::Codex, &path, false).unwrap();
    // An index from the previous build has records but no generation table.
    i.db.execute_batch("DROP TABLE log_generations").unwrap();
    drop(i);
    let mut i = f.index();
    assert_eq!(i.cached().unwrap()[0].cli_id, "one");
    let original = i.chat_generation(path.to_str().unwrap()).unwrap();
    assert_eq!(original, "", "old records need no backfill");
    fs::write(&path, line("x")).unwrap();
    let truncated = i.update(Agent::Codex, &path, false).unwrap().unwrap();
    assert_eq!(truncated.cli_id, "x");
    let truncated = i.chat_generation(path.to_str().unwrap()).unwrap();
    assert_ne!(original, truncated);
    let replacement = f.0.join("replacement");
    fs::write(&replacement, line("replacement")).unwrap();
    fs::rename(replacement, &path).unwrap();
    let replaced = i.update(Agent::Codex, &path, false).unwrap().unwrap();
    assert_eq!(replaced.cli_id, "replacement");
    assert_ne!(
        truncated,
        i.chat_generation(path.to_str().unwrap()).unwrap()
    );
}
