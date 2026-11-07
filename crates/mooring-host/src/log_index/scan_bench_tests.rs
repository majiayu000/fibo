use super::*;

/// What a host pays to catch up after a restart where the system keeps no journal: a look
/// through the whole log tree with nothing changed. Run by hand, `MOORING_SCAN_BENCH=dirs,files`:
/// `cargo test -p mooring-host --lib scan_bench -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, by hand"]
fn scan_bench() {
    let (dirs, files) = std::env::var("MOORING_SCAN_BENCH")
        .ok()
        .and_then(|v| {
            let (d, f) = v.split_once(',')?;
            Some((d.parse::<usize>().ok()?, f.parse::<usize>().ok()?))
        })
        .unwrap_or((2208, 6402));
    let f = Fixture::new();
    let logs = f.0.join("logs");
    let body = line("x").repeat(20);
    for i in 0..files {
        let dir = logs.join(format!("d{}/e{}", i % 40, i % dirs));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{i}.jsonl")), &body).unwrap();
    }
    let mut i = f.index();
    let since = i.initialize().unwrap();
    let at = std::time::Instant::now();
    let watch = Watch::start(std::slice::from_ref(&logs), since).unwrap();
    eprintln!("watching {dirs} folders took {:?}", at.elapsed());
    let at = std::time::Instant::now();
    i.bootstrap().unwrap();
    eprintln!("first import took {:?} ({:?})", at.elapsed(), i.counts);
    drop(watch);
    for round in 0..3 {
        i.counts = IoCounts::default();
        let scopes = [logs.clone()];
        i.queue_recovery(&scopes).unwrap();
        let at = std::time::Instant::now();
        let mut recovery = Recovery::new(&i, &scopes);
        let mut steps = 0;
        while !recovery.step(&mut i).unwrap().0 {
            steps += 1;
        }
        eprintln!(
            "look {round}: {:?}, {steps} steps, nothing changed ({:?})",
            at.elapsed(),
            i.counts
        );
    }
}

/// What a first start costs on this account's own logs: the index goes in a directory of its own,
/// and the logs are only read. `/usr/bin/time -l` around it gives the peak memory. By hand, in a
/// release build: `cargo test --release -p mooring-host --lib real_first_import -- --ignored
/// --nocapture`.
#[test]
#[ignore = "a measurement, by hand; reads every log this account has"]
fn real_first_import() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let root = std::env::temp_dir().join(format!("mooring-import-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let roots = vec![
        (Agent::Claude, home.join(".claude/projects")),
        (Agent::Codex, home.join(".codex/sessions")),
    ];
    let mut i = Index::open(&root, roots.clone()).unwrap();
    i.initialize().unwrap();
    let at = std::time::Instant::now();
    i.bootstrap().unwrap();
    eprintln!("first import took {:?} ({:?})", at.elapsed(), i.counts);
    let scopes: Vec<_> = roots.into_iter().map(|(_, p)| p).collect();
    i.counts = IoCounts::default();
    i.queue_recovery(&scopes).unwrap();
    let at = std::time::Instant::now();
    let mut recovery = Recovery::new(&i, &scopes);
    while !recovery.step(&mut i).unwrap().0 {}
    eprintln!("look again: {:?} ({:?})", at.elapsed(), i.counts);
    drop(i);
    let size: u64 = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.metadata().map_or(0, |m| m.len()))
        .sum();
    eprintln!("index on disk: {} MB", size / 1_000_000);
    fs::remove_dir_all(&root).unwrap();
}

/// A CLI nobody uses here has no log folder, and that is not an outage. A folder that held logs
/// and went away is, and its logs are forgotten once repaired. Paths use this system's separator.
#[test]
fn only_a_folder_that_held_logs_is_lost_when_gone() {
    let f = Fixture::new();
    let logs = f.0.join("logs");
    let used = logs.join("used");
    fs::create_dir_all(&used).unwrap();
    let log = used.join("a.jsonl");
    fs::write(&log, line("x").repeat(2)).unwrap();
    let mut i = f.index();
    i.initialize().unwrap();
    i.bootstrap().unwrap();
    assert_eq!(i.cached().unwrap().len(), 1);

    assert!(!i.lost_logs(&[logs.join("never-there")]).unwrap());
    assert!(
        !i.lost_logs(std::slice::from_ref(&used)).unwrap(),
        "still there"
    );

    fs::remove_dir_all(&used).unwrap();
    assert!(i.lost_logs(std::slice::from_ref(&used)).unwrap());
    i.queue_recovery(std::slice::from_ref(&used)).unwrap();
    i.finish_recovery(std::slice::from_ref(&used)).unwrap();
    assert!(i.cached().unwrap().is_empty());
    assert!(!i.lost_logs(std::slice::from_ref(&used)).unwrap());
}
