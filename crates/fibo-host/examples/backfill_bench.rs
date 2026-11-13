//! What reading a long log back costs: time per call, peak memory and what came of it. Prints
//! numbers only, never the log's contents, and writes only under a temporary directory; the
//! log itself is only opened for reading.
//! `cargo run --release -p fibo-host --example backfill_bench -- <claude|codex> <log>`
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
    let dir = std::env::temp_dir().join(format!("fibo-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let root = log.parent().expect("a log in a directory").to_owned();
    let mut index = Index::open(&dir, vec![(agent, root)])?;
    let size = std::fs::metadata(&log)?.len();
    index.update(agent, &log, true)?;
    index.flush()?;
    let path = log.to_string_lossy();
    let origin = index.card(&path)?.map_or(0, |c| c.from_offset);
    let first = index.chat(&path, origin, None, None, 500, false)?.0.len();
    println!(
        "{} MB, first read: {first} messages, peak {} MB",
        size >> 20,
        peak_mb()
    );
    let (began, mut calls, mut slowest, mut messages) = (Instant::now(), 0, 0f64, 0u64);
    let mut first_page = None;
    loop {
        let t = Instant::now();
        let got = index.backfill(agent, &path)?;
        let took = t.elapsed().as_secs_f64() * 1000.0;
        slowest = slowest.max(took);
        calls += 1;
        messages += u64::from(got.added);
        if first_page.is_none() && got.added > 0 {
            first_page = Some(began.elapsed().as_secs_f64() * 1000.0);
        }
        if got.done {
            break;
        }
    }
    println!(
        "read back in {:.1} s: {calls} calls, slowest {slowest:.0} ms, first messages after {:.0} ms, {messages} messages found, peak {} MB",
        began.elapsed().as_secs_f64(),
        first_page.unwrap_or(0.0),
        peak_mb()
    );
    // Paging the whole conversation back, as a phone scrolling up would.
    let (t, mut pages, mut before, mut shown) = (Instant::now(), 0, None, 0);
    let mut last_seq = i64::MAX;
    let mut ordered = true;
    loop {
        let (page, more) = index.chat(&path, 0, None, before, 50, false)?;
        pages += 1;
        shown += page.len();
        ordered &= page.iter().all(|m| m.seq < last_seq);
        last_seq = page.first().map_or(last_seq, |m| m.seq);
        before = page.first().map(|m| m.seq);
        if !more {
            break;
        }
    }
    println!(
        "{shown} messages in {pages} pages, {:.1} ms per page, order holds: {ordered}",
        t.elapsed().as_secs_f64() * 1000.0 / f64::from(pages),
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
