use mooring_platform::system::{cpu_ticks, memory};
use std::time::{Duration, Instant};

#[test]
fn memory_adds_up() {
    let m = memory().expect("every platform answers this");
    assert!(m.total >= 1 << 26, "{m:?}");
    assert!(m.used > 0 && m.used <= m.total, "{m:?}");
    assert!(m.cached <= m.total, "{m:?}");
}

#[test]
fn cpu_time_moves_forward() {
    let first = cpu_ticks().unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let now = cpu_ticks().unwrap();
        let moved: u64 = (0..4)
            .map(|i| u64::from(now[i].wrapping_sub(first[i])))
            .sum();
        if moved > 0 {
            return;
        }
        assert!(Instant::now() < until, "still {now:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn this_machine_has_a_name() {
    assert!(!mooring_platform::system::host_name().is_empty());
}
