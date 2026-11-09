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

fn steps(index: &mut Index, read: impl FnOnce(&Index)) -> usize {
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
    read(index);
    index.db.progress_handler(0, None::<fn() -> bool>).unwrap();
    ticks.load(Ordering::Relaxed)
}

#[test]
fn an_empty_poll_does_not_walk_old_records() {
    let (_fixture, mut index) = populated();
    let work = steps(&mut index, |index| {
        let (messages, more) = index
            .chat("bench", 0, Some(5_000_000 * 4096 + 1), None, 50, false)
            .unwrap();
        assert!(messages.is_empty() && !more);
    });
    assert!(work < 1000, "an empty poll executed about {work} VM steps");
}

#[test]
fn latest_messages_skip_a_long_non_message_tail() {
    let (_fixture, mut index) = populated();
    let work = steps(&mut index, |index| {
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
    let (_fixture, mut index) = populated();
    let work = steps(&mut index, |index| {
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

fn old_tools(failed: bool) -> (Fixture, Index) {
    let (fixture, index) = populated();
    index.db.execute_batch("UPDATE session_records SET kind='tool_result',call_id='later-'||offset,exit_code=1 WHERE offset>=10000;
        UPDATE session_records SET text='Edit {\"file_path\":\"/r/a\",\"old_string\":\"x\",\"new_string\":\"y\"}' WHERE kind='tool_call';").unwrap();
    // The failure is beyond both the old page and its first 32 results.
    for n in 0..40 {
        index
            .db
            .execute(
                "INSERT INTO session_records VALUES('bench',1,?1,0,0,'tool_result','','tool-1',?2)",
                params![
                    5_100_000 + n * 100,
                    if failed && n == 39 { Some(1) } else { None }
                ],
            )
            .unwrap();
    }
    (fixture, index)
}

#[test]
fn old_tool_pages_skip_unrelated_later_results() {
    for failed in [false, true] {
        let (_fixture, mut index) = old_tools(failed);
        let work = steps(&mut index, |index| {
            let (summary, rows, next) = index
                .tool_calls("bench", Some(1000 * 4096 + 1), None, 1)
                .unwrap();
            assert_eq!(rows.len(), 1);
            assert!(next.is_none());
            assert_eq!(rows[0].results.len(), 32);
            assert_eq!(rows[0].results[0].offset, 200);
            assert_eq!(rows[0].results[31].offset, 5_103_000);
            assert_eq!(summary.unwrap().files.len(), usize::from(!failed));
        });
        assert!(work < 10000, "old tool page executed about {work} VM steps");
    }
}

#[test]
fn old_chat_summaries_skip_unrelated_later_results() {
    for failed in [false, true] {
        let (_fixture, mut index) = old_tools(failed);
        let work = steps(&mut index, |index| {
            let (messages, _) = index
                .chat("bench", 0, None, Some(1000 * 4096 + 2), 1, true)
                .unwrap();
            assert_eq!(messages.len(), 1);
            let summary = messages[0].tools.as_ref().unwrap();
            assert_eq!(summary.calls, 1);
            assert_eq!(summary.files.len(), usize::from(!failed));
        });
        assert!(
            work < 10000,
            "old chat summary executed about {work} VM steps"
        );
    }
}

#[test]
fn an_empty_tool_page_skips_a_result_only_tail() {
    let (_fixture, mut index) = populated();
    index.db.execute_batch("UPDATE session_records SET kind='tool_result',call_id='later-'||offset WHERE offset>=10000;").unwrap();
    let work = steps(&mut index, |index| {
        let (_, rows, next) = index
            .tool_calls("bench", None, Some(9000 * 4096 + 1), 50)
            .unwrap();
        assert!(rows.is_empty() && next.is_none());
    });
    assert!(
        work < 10000,
        "empty tool page executed about {work} VM steps"
    );
}

#[test]
fn tool_page_cursors_keep_calls_at_the_same_log_offset() {
    let fixture = Fixture::new();
    let index = fixture.index();
    // Deliberately not inserted in log order, as happens when older lines are read back.
    index
        .db
        .execute_batch(
            "INSERT INTO session_records VALUES
        ('bench',1,200,0,2,'assistant','done','',NULL),
        ('bench',1,0,0,0,'user','go','',NULL),
        ('bench',1,300,0,3,'tool_result','','one',NULL),
        ('bench',1,300,1,3,'tool_result','','two',NULL),
        ('bench',1,100,1,1,'tool_call','Bash {}','two',NULL),
        ('bench',1,100,0,1,'tool_call','Bash {}','one',NULL);",
        )
        .unwrap();
    let until = Some(200 * 4096 + 1);
    let (summary, first, next) = index.tool_calls("bench", until, None, 1).unwrap();
    assert_eq!(summary.unwrap().calls, 2);
    assert_eq!(first[0].id, "one");
    assert_eq!(next, Some(100 * 4096 + 1));
    assert_eq!(first[0].results[0].seq, 300 * 4096 + 1);
    let (summary, second, next) = index.tool_calls("bench", until, next, 1).unwrap();
    assert!(summary.is_none() && next.is_none());
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, "two");
    assert_eq!(second[0].results[0].seq, 300 * 4096 + 2);
    let (_, empty, next) = index
        .tool_calls("bench", until, Some(second[0].seq), 1)
        .unwrap();
    assert!(empty.is_empty() && next.is_none());
}
