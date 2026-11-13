//! cargo run --release -p fibo-core --example search_bench
//! Synthetic bounded VT history; text allocation bytes are not process RSS.
use fibo_core::terminal::Vt;
use std::{collections::HashMap, hint::black_box, time::Instant};

fn main() {
    let mut vt = Vt::new(200, 24).unwrap();
    vt.feed(("a".repeat(199) + "\r\n").repeat(2000).as_bytes());
    let found = vt.search("a").unwrap();
    let mut lines = HashMap::new();
    for m in &found {
        lines
            .entry(m.line.as_ptr() as usize)
            .or_insert(m.line.len());
    }
    let mut times = Vec::new();
    for _ in 0..7 {
        let began = Instant::now();
        black_box(vt.search("a").unwrap());
        times.push(began.elapsed());
    }
    times.sort();
    println!(
        "hits={} unique_lines={} text_bytes={} median_ms={:.3}",
        found.len(),
        lines.len(),
        lines.values().sum::<usize>(),
        times[3].as_secs_f64() * 1000.
    );
}
