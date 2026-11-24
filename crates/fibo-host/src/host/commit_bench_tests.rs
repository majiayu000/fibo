//! What a commit that only moves the revision costs, on a copy of a real install's store.
//! `MOORING_REFRESH_BENCH=<dir holding a copy of workbench.sqlite> cargo test --release -p fibo-host --lib commit_bench -- --ignored --nocapture`.
use super::*;

#[test]
#[ignore]
fn commit_bench() {
    let root = std::path::PathBuf::from(std::env::var("MOORING_REFRESH_BENCH").unwrap());
    let (mut store, mut w) = crate::store::Store::open(&root).unwrap();
    fibo_core::retention::archive(&mut w, fibo_core::content::now(), &Default::default());
    for _ in 0..5 {
        let t = Instant::now();
        let mut next = w.clone();
        let clone = t.elapsed();
        next.revision += 1;
        let t = Instant::now();
        next.settle_starts();
        let settle = t.elapsed();
        let t = Instant::now();
        store.save(&w, &next).unwrap();
        let save = t.elapsed();
        let t = Instant::now();
        let notices = events::Tracker::snapshot(&next);
        let events = t.elapsed();
        println!(
            "clone {clone:?} settle {settle:?} save {save:?} events {events:?} ({} notices)",
            notices.len()
        );
        w = next;
    }
}
