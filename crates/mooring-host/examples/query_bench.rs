//! cargo run --release -p mooring-host --example query_bench
//! Synthetic data in fresh temporary databases. Compare source revisions for query
//! changes; each run also isolates the current indexes' build, read and append costs.
use mooring_host::{content, log_index::Index};
use rusqlite::{params, Connection};
use serde_json::json;
use std::{hint::black_box, time::Instant};

fn median(mut f: impl FnMut()) -> f64 {
    let mut times = Vec::new();
    for _ in 0..7 {
        let began = Instant::now();
        f();
        times.push(began.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    times[3]
}
fn insert(db: &Connection, start: i64, count: i64) -> f64 {
    let began = Instant::now();
    db.execute_batch("BEGIN").unwrap();
    let mut q = db
        .prepare("INSERT INTO session_records VALUES('bench',1,?1,0,?1,?2,?3,?4,?5)")
        .unwrap();
    for n in start..start + count {
        let kind = match n % 10 {
            0 => {
                if n % 20 == 0 {
                    "user"
                } else {
                    "assistant"
                }
            }
            1 | 3 | 5 | 7 => "tool_call",
            2 | 4 | 6 | 8 => "tool_result",
            _ => "other",
        };
        let id = match kind {
            "tool_call" => format!("call-{}", n / 2),
            "tool_result" => format!("call-{}", (n - 1) / 2),
            _ => String::new(),
        };
        let text = if kind == "tool_call" {
            "Bash {\"command\":\"true\"}".to_owned()
        } else {
            format!("message-{n}")
        };
        q.execute(params![
            n * 100,
            kind,
            text,
            id,
            if n % 34 == 0 { Some(1) } else { None }
        ])
        .unwrap();
    }
    drop(q);
    db.execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    began.elapsed().as_secs_f64() * 1000.
}
fn main() {
    for indexed in [false, true] {
        let root =
            std::env::temp_dir().join(format!("mooring-query-bench-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let index = Index::open(&root, vec![]).unwrap();
        let file = root.join("workbench.sqlite");
        let db = Connection::open(&file).unwrap();
        db.execute_batch(
            "PRAGMA synchronous=NORMAL;
            DROP INDEX IF EXISTS session_records_messages;
            DROP INDEX IF EXISTS session_records_call;",
        )
        .unwrap();
        let seed = insert(&db, 0, 500000);
        let began = Instant::now();
        if indexed {
            content::schema(&db).unwrap();
        }
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
        let build = began.elapsed().as_secs_f64() * 1000.;
        let bytes = std::fs::metadata(&file).unwrap().len();
        let latest = median(|| {
            black_box(index.chat("bench", 0, None, None, 50, false).unwrap());
        });
        let empty = median(|| {
            black_box(
                index
                    .chat("bench", 0, Some(50_000_000 * 4096 + 1), None, 50, false)
                    .unwrap(),
            );
        });
        let detail = median(|| {
            black_box(index.tool_call("bench", "call-0").unwrap());
        });
        let page = median(|| {
            black_box(
                index
                    .tool_calls("bench", Some(100_000 * 4096 + 1), None, 50)
                    .unwrap(),
            );
        });
        let append = insert(&db, 500000, 10000);
        println!(
            "{}",
            json!({"indexed":indexed,"rows":500000,"db_bytes":bytes,"seed_ms":seed,
            "index_build_ms":build,"latest_ms":latest,"empty_poll_ms":empty,"detail_ms":detail,
            "old_tools_ms":page,"append_10k_ms":append})
        );
        drop(db);
        drop(index);
        std::fs::remove_dir_all(root).unwrap();
    }
}
