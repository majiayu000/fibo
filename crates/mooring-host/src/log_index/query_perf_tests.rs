//! Count SQLite VM work on the real Index methods, independent of wall-clock load.
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn populated() -> (Fixture, Index) {
    let fixture = Fixture::new();
    let index = fixture.index();
    index.db.execute_batch("WITH RECURSIVE rows(n) AS (VALUES(0) UNION ALL SELECT n+1 FROM rows WHERE n<49999)
        INSERT INTO session_records SELECT 'bench',1,n*100,0,n,
        CASE WHEN n=1 THEN 'tool_call' WHEN n=2 THEN 'tool_result' WHEN n<100 AND n%10=0 THEN 'assistant' ELSE 'other' END,
        CASE WHEN n=1 THEN 'Bash {\"command\":\"true\"}' ELSE 'message-'||n END,
        CASE WHEN n IN (1,2) THEN 'tool-1' ELSE '' END, NULL FROM rows;").unwrap();
    (fixture, index)
}

fn steps(index: &Index, read: impl FnOnce()) -> usize {
    let ticks = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&ticks);
    index
        .db
        .progress_handler(
            100,
            Some(move || {
                count.fetch_add(100, Ordering::Relaxed);
                false
            }),
        )
        .unwrap();
    read();
    index.db.progress_handler(0, None::<fn() -> bool>).unwrap();
    ticks.load(Ordering::Relaxed)
}

#[test]
fn an_empty_poll_does_not_walk_old_records() {
    let (_fixture, index) = populated();
    let work = steps(&index, || {
        let (messages, more) = index
            .chat("bench", 0, Some(5_000_000 * 4096 + 1), None, 50, false)
            .unwrap();
        assert!(messages.is_empty() && !more);
    });
    assert!(work < 1000, "an empty poll executed about {work} VM steps");
}

#[test]
fn latest_messages_skip_a_long_non_message_tail() {
    let (_fixture, index) = populated();
    let work = steps(&index, || {
        let (messages, more) = index.chat("bench", 0, None, None, 50, false).unwrap();
        assert_eq!(messages.len(), 10);
        assert_eq!(messages[0].text, "message-0");
        assert!(!more);
    });
    assert!(
        work < 2000,
        "the latest messages executed about {work} VM steps"
    );
}

#[test]
fn a_single_call_does_not_walk_unrelated_records() {
    let (_fixture, index) = populated();
    let work = steps(&index, || {
        let call = index.tool_call("bench", "tool-1").unwrap().unwrap();
        assert_eq!(call.results.len(), 1);
        assert_eq!(call.results[0].offset, 200);
    });
    assert!(work < 1000, "a single call executed about {work} VM steps");
}

#[test]
fn advancing_the_poll_floor_keeps_ordinals_and_the_history_floor() {
    let fixture = Fixture::new();
    let index = fixture.index();
    index
        .db
        .execute_batch(
            "INSERT INTO session_records VALUES
        ('bench',1,100,0,1,'assistant','first','',NULL),
        ('bench',1,100,1,2,'assistant','second','',NULL),
        ('bench',1,200,0,3,'assistant','later','',NULL);",
        )
        .unwrap();
    let (messages, _) = index
        .chat("bench", 0, Some(100 * 4096 + 1), None, 50, false)
        .unwrap();
    assert_eq!(
        messages.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
        ["second", "later"]
    );
    let (messages, _) = index
        .chat("bench", 200, Some(100 * 4096 + 1), None, 50, false)
        .unwrap();
    assert_eq!(
        messages.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
        ["later"]
    );
}
