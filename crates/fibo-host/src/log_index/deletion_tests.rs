use super::*;

const TABLES: [&str; 6] = [
    "log_files",
    "session_content",
    "session_records",
    "subagent_links",
    "log_backfill",
    "log_generations",
];

fn conversation(path: &Path, id: &str, message: &str) {
    let now = fibo_core::content::now();
    fs::write(
        path,
        format!(
            "{}{}\n",
            line(id),
            serde_json::json!({
                "type": "event_msg",
                "timestamp": now,
                "payload": {"type": "user_message", "message": message}
            })
        ),
    )
    .unwrap();
}

fn indexed_rows(index: &Index, table: &str, path: &Path) -> i64 {
    index
        .db
        .query_row(
            &format!("SELECT count(*) FROM {table} WHERE path=?1"),
            [path.to_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn a_removed_log_loses_its_durable_and_held_content_without_touching_another_path() {
    let fixture = Fixture::new();
    let mut index = fixture.index();
    index.initialize().unwrap();
    let gone = fixture.0.join("logs/gone.jsonl");
    let kept = fixture.0.join("logs/kept.jsonl");
    conversation(&gone, "same-id", "removed test prompt");
    conversation(&kept, "same-id", "kept test prompt");
    index.update_many([gone.clone(), kept.clone()], 1).unwrap();

    for path in [&gone, &kept] {
        let key = path.to_str().unwrap();
        index
            .db
            .execute("INSERT INTO log_backfill VALUES (?1,1,2,3)", [key])
            .unwrap();
        index
            .db
            .execute(
                "INSERT INTO subagent_links VALUES (?1,'missing-parent','worker','Running','',0)",
                [key],
            )
            .unwrap();
        for table in TABLES {
            assert!(indexed_rows(&index, table, path) > 0, "{table}");
        }
    }
    let key = gone.to_str().unwrap();
    let old_generation = index.chat_generation(key).unwrap();
    let reader = index.fork_current().unwrap();
    assert!(reader.card(key).unwrap().is_some());
    assert!(!reader
        .chat(key, 0, None, None, 5, false)
        .unwrap()
        .0
        .is_empty());

    fs::remove_file(&gone).unwrap();
    index.update_many([gone.clone()], 2).unwrap();
    index.flush().unwrap();
    for table in TABLES {
        assert_eq!(indexed_rows(&reader, table, &gone), 0, "{table}");
        assert!(indexed_rows(&reader, table, &kept) > 0, "{table}");
    }
    reader.answered(Agent::Codex, "same-id", key).unwrap();
    assert!(reader.card(key).unwrap().is_none());
    assert_ne!(reader.chat_generation(key).unwrap(), old_generation);
    assert!(reader
        .chat(key, 0, None, None, 5, false)
        .unwrap()
        .0
        .is_empty());
    assert_eq!(reader.cached().unwrap()[0].path, kept.to_str().unwrap());
}

#[test]
fn a_temporarily_missing_root_keeps_its_cached_logs() {
    let fixture = Fixture::new();
    let mut index = fixture.index();
    index.initialize().unwrap();
    let root = fixture.0.join("logs");
    let log = root.join("a.jsonl");
    conversation(&log, "a", "still available");
    index.update_many([log.clone()], 1).unwrap();
    fs::rename(&root, fixture.0.join("offline")).unwrap();
    index.update_many([log.clone()], 2).unwrap();
    index.finish_recovery(&[root]).unwrap();
    assert!(index.card(log.to_str().unwrap()).unwrap().is_some());
    assert_eq!(indexed_rows(&index, "log_files", &log), 1);
}

#[test]
#[cfg(unix)]
fn an_unreadable_log_is_not_deleted_from_the_index() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let mut index = fixture.index();
    let dir = fixture.0.join("logs").join("private");
    let log = dir.join("a.jsonl");
    fs::create_dir(&dir).unwrap();
    conversation(&log, "a", "still cached");
    index.update(Agent::Codex, &log, true).unwrap();
    let original = fs::metadata(&dir).unwrap().permissions();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
    let result = index.update(Agent::Codex, &log, true);
    fs::set_permissions(&dir, original).unwrap();
    if let Err(error) = result {
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }
    assert!(index.card(log.to_str().unwrap()).unwrap().is_some());
    assert_eq!(indexed_rows(&index, "log_files", &log), 1);
}

#[test]
fn recovery_does_not_delete_anything_outside_its_scope() {
    let fixture = Fixture::new();
    let mut index = fixture.index();
    let scope = fixture.0.join("logs/gone");
    let gone = scope.join("a.jsonl");
    let outside = fixture.0.join("logs/other.jsonl");
    fs::create_dir(&scope).unwrap();
    conversation(&gone, "a", "within repaired scope");
    conversation(&outside, "b", "outside repaired scope");
    index.initialize().unwrap();
    index.bootstrap().unwrap();
    fs::remove_dir_all(&scope).unwrap();
    fs::remove_file(&outside).unwrap();
    index.finish_recovery(&[scope]).unwrap();
    assert!(index.card(gone.to_str().unwrap()).unwrap().is_none());
    assert_eq!(indexed_rows(&index, "session_records", &gone), 0);
    assert!(index.card(outside.to_str().unwrap()).unwrap().is_some());
}

#[test]
fn conversation_identity_and_chat_share_a_read_snapshot() {
    let fixture = Fixture::new();
    let mut index = fixture.index();
    let log = fixture.0.join("logs/shared.jsonl");
    let key = log.to_str().unwrap();
    conversation(&log, "old", "old message");
    index.update(Agent::Codex, &log, true).unwrap();
    let reader = index.fork_current().unwrap();
    let messages = reader
        .read_for(Agent::Codex, "old", key, |reader| {
            let new = fixture.0.join("new.jsonl");
            conversation(&new, "new", "new message");
            fs::rename(new, &log).unwrap();
            index.update(Agent::Codex, &log, true)?;
            Ok(reader.chat(key, 0, None, None, 5, false)?.0)
        })
        .unwrap()
        .unwrap();
    assert_eq!(messages[0].text, "old message");
    assert!(reader
        .read_for(Agent::Codex, "old", key, |_| Ok(()))
        .unwrap()
        .is_none());
    assert_eq!(
        reader.chat(key, 0, None, None, 5, false).unwrap().0[0].text,
        "new message"
    );
}

#[test]
fn cards_from_old_orphan_rows_stay_hidden_after_restart() {
    let fixture = Fixture::new();
    let mut index = fixture.index();
    let log = fixture.0.join("logs/old.jsonl");
    let key = log.to_str().unwrap();
    conversation(&log, "old", "historical fixture");
    index.update(Agent::Codex, &log, true).unwrap();
    assert!(index.card(key).unwrap().is_some());
    // This is what an older index did on deletion.
    index
        .db
        .execute("DELETE FROM log_files WHERE path=?1", [key])
        .unwrap();
    drop(index);

    let reader = fixture.index();
    assert!(reader.card(key).unwrap().is_none());
    assert!(reader
        .read_for(Agent::Codex, "old", key, |_| Ok(()))
        .unwrap()
        .is_none());
    assert_eq!(indexed_rows(&reader, "session_content", &log), 1);
}
