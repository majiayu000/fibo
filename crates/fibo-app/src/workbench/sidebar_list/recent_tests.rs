use super::*;

#[test]
fn what_needs_you_leads_then_running_then_stopped_and_quiet_last() {
    let rows = [
        ("old-review", Bucket::Done, 10),
        ("asking", Bucket::Attention, 5),
        ("running", Bucket::Running, 30),
        ("quiet", Bucket::Stale, 1),
        ("failed", Bucket::Attention, 40),
        ("new-review", Bucket::Done, 50),
    ];
    let order = recent_order(&rows);
    assert_eq!(order.need, ["failed", "asking", "new-review", "old-review"]);
    assert_eq!(order.running, ["running"]);
    assert!(order.stopped.is_empty());
    assert_eq!(order.quiet, ["quiet"]);
}

#[test]
fn history_splits_at_a_day_newest_first() {
    let now = 10 * 86_400;
    let rows = vec![
        ("two-days", now - 2 * 86_400),
        ("hour", now - 3_600),
        ("minute", now - 60),
        ("day-and-a-bit", now - 86_400),
    ];
    let (today, older) = split_today(rows, now);
    assert_eq!(today, ["minute", "hour"]);
    assert_eq!(older, ["day-and-a-bit", "two-days"]);
}
