use super::*;

/// What `cached()` then keeping `updated >= since` gives (callers sort it themselves), for
/// rows only in the database, logs only in memory, and memory newer or older than the row.
#[test]
fn since_gives_what_cached_then_a_filter_gives() {
    let f = Fixture::new();
    let logs = f.0.join("logs");
    for id in ["row", "held", "ahead", "back"] {
        fs::write(logs.join(format!("{id}.jsonl")), line(id)).unwrap();
    }
    let mut i = f.index();
    i.initialize().unwrap();
    i.bootstrap().unwrap();
    let set = |i: &Index, path: &str, updated: u64| {
        let path = logs.join(path).to_string_lossy().into_owned();
        i.db.execute(
            "UPDATE log_files SET updated=?2 WHERE path=?1",
            params![path, updated as i64],
        )
        .unwrap();
    };
    let now = fibo_core::content::now();
    // Held in memory as a moving log is, updated now there, its row then set apart:
    // "held"'s row is older than memory, "ahead"'s newer, and "only" has none.
    let ask =
        "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"再看看\"}}\n";
    let moving: Vec<_> = ["held", "ahead", "only"]
        .iter()
        .map(|id| logs.join(format!("{id}.jsonl")))
        .collect();
    for path in &moving {
        fs::write(
            path,
            format!("{}{ask}", line(path.file_stem().unwrap().to_str().unwrap())),
        )
        .unwrap();
    }
    i.update_many(moving.clone(), 1).unwrap();
    set(&i, "row.jsonl", 500);
    set(&i, "held.jsonl", 100);
    set(&i, "ahead.jsonl", now + 1000);
    set(&i, "back.jsonl", 900);
    (i.db)
        .execute(
            "DELETE FROM log_files WHERE path=?1",
            [moving[2].to_string_lossy()],
        )
        .unwrap();
    let all = i.cached().unwrap();
    assert_eq!(all.len(), 5, "{all:?}");
    assert!(
        all.iter().all(|t| t.updated <= now + 1),
        "memory outranks rows: {all:?}"
    );
    for since in [0, 100, 501, 901, now - 5, now + 5] {
        let mut filtered: Vec<_> = (all.iter())
            .filter(|t| t.updated >= since)
            .map(|t| (t.path.clone(), t.updated))
            .collect();
        let mut got: Vec<_> = (i.cached_since(since).unwrap().into_iter())
            .map(|t| (t.path, t.updated))
            .collect();
        filtered.sort();
        got.sort();
        assert_eq!(got, filtered, "since {since}");
    }
}

/// What `follow_associated` paid per log batch while an Agent was unbound: every log ever
/// identified, against the last ten minutes; and the whole read `refresh_agents` still
/// makes. 20 logs moving (held in memory) and 20 more written lately. By hand,
/// `MOORING_CACHED_BENCH=<rows>`: `cargo test --release -p fibo-host --lib
/// cached_since_bench -- --ignored --nocapture --test-threads=1`.
#[test]
#[ignore = "a measurement, by hand"]
fn cached_since_bench() {
    let rows: usize = std::env::var("MOORING_CACHED_BENCH").map_or(34_000, |v| v.parse().unwrap());
    let f = Fixture::new();
    let mut i = f.index();
    let now = fibo_core::content::now() as i64;
    let tx = i.db.unchecked_transaction().unwrap();
    for n in 0..rows {
        let updated = if n % (rows / 20) == 0 {
            now - 60
        } else {
            now - 86400 * (1 + n as i64 % 90)
        };
        tx.execute(
            "INSERT INTO log_files(path,runtime,dev,inode,offset,pending,discard,cli_id,cwd,updated) VALUES(?1,'codex',1,?2,4096,x'',0,?3,?4,?5)",
            params![
                format!("/Users/someone/.codex/sessions/2026/11/{:02}/rollout-2026-11-{:02}T10-00-00-019a{n:08}-5c1e-7d40-9b8e.jsonl", n % 28 + 1, n % 28 + 1),
                n as i64,
                format!("019a{n:08}-5c1e-7d40-9b8e-2f6a3c1d0000"),
                format!("/Users/someone/code/project-{}", n % 40),
                updated
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let moving: Vec<_> = (0..20)
        .map(|k| f.0.join(format!("logs/moving-{k}.jsonl")))
        .collect();
    for (k, path) in moving.iter().enumerate() {
        fs::write(path, line(&format!("moving-{k}"))).unwrap();
    }
    i.update_many(moving, 1).unwrap();
    let since = (now - 600) as u64;
    let recent = (i.cached().unwrap().iter())
        .filter(|t| t.updated >= since)
        .count();
    assert_eq!(recent, 40);
    let instructions =
        || fibo_platform::inspect::usage(std::process::id()).map_or(0, |u| u.instructions);
    for round in 0..3 {
        for (name, f, n) in [
            (
                "cached",
                &(|| i.cached().unwrap().len()) as &dyn Fn() -> usize,
                rows + 20,
            ),
            (
                "cached then filter",
                &|| {
                    (i.cached().unwrap().iter())
                        .filter(|t| t.updated >= since)
                        .count()
                },
                recent,
            ),
            (
                "cached_since",
                &|| i.cached_since(since).unwrap().len(),
                recent,
            ),
        ] {
            let at = instructions();
            for _ in 0..20 {
                assert_eq!(std::hint::black_box(f()), n);
            }
            let per = (instructions() - at) / 20;
            eprintln!("round {round} {name}: {:.2} M per call", per as f64 / 1e6);
        }
    }
}
