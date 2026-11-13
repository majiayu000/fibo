//! What reading one log into the index costs: time, peak memory and what came of it. Prints
//! numbers only, never the log's contents, and writes only under a temporary directory.
//! `cargo run --release -p fibo-host --example index_bench -- <claude|codex> <log> [first]`;
//! `first` reads it as the first walk does (`historical`), otherwise as a moving log.
use fibo_core::model::Agent;
use fibo_host::log_index::Index;
use std::path::PathBuf;
use std::time::Instant;

fn peak_mb() -> u64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `usage` is a valid out pointer for the call's duration.
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    // Bytes on macOS.
    usage.ru_maxrss as u64 >> 20
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let agent = match args.next().as_deref() {
        Some("codex") => Agent::Codex,
        _ => Agent::Claude,
    };
    let log = PathBuf::from(args.next().expect("a log path"));
    let historical = args.next().as_deref() == Some("first");
    let dir = std::env::temp_dir().join(format!("fibo-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let root = log.parent().expect("a log in a directory").to_owned();
    let mut index = Index::open(&dir, vec![(agent, root)])?;
    let size = std::fs::metadata(&log)?.len();
    let began = Instant::now();
    let found = index.update(agent, &log, historical)?;
    index.flush()?;
    let took = began.elapsed().as_secs_f64();
    let path = log.to_string_lossy();
    let (messages, _) = index.chat(&path, 0, None, None, 500, false)?;
    let card = index.card(&path)?;
    println!(
        "{} MB in {took:.1} s ({:.0} MB/s), peak {} MB, read {} MB; found={}, newest-500 chat messages={}, partial_history={:?}, unknown_records={:?}",
        size >> 20,
        (size >> 20) as f64 / took.max(0.001),
        peak_mb(),
        index.counts.bytes >> 20,
        found.is_some(),
        messages.len(),
        card.as_ref().map(|c| c.partial_history),
        card.as_ref().map(|c| c.unknown_records),
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
