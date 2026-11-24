use super::*;

#[test]
fn what_needs_you_leads_then_running_then_stopped_and_quiet_last() {
    let rows = [
        ("old-review", Bucket::Attention, Status::Review, 10),
        ("asking", Bucket::Attention, Status::Waiting, 5),
        ("running", Bucket::Running, Status::Running, 30),
        ("quiet", Bucket::Stale, Status::Stale, 1),
        ("failed", Bucket::Attention, Status::Error, 40),
        ("new-review", Bucket::Attention, Status::Review, 50),
    ];
    let order = recent_order(&rows);
    assert_eq!(order.need, ["asking", "failed", "new-review", "old-review"]);
    assert_eq!(order.running, ["running"]);
    assert!(order.stopped.is_empty());
    assert_eq!(order.quiet, ["quiet"]);
}

/// 要你处理 holds asking, failed, finished and 等后台 rows in one bucket (11-03); a
/// finished turn that moved last must still not sit above one waiting for your answer.
#[test]
fn inside_needs_you_what_blocks_comes_before_a_newer_finished_turn() {
    let rows = [
        ("background", Bucket::Attention, Status::Background, 90),
        ("review", Bucket::Attention, Status::Review, 80),
        ("asking", Bucket::Attention, Status::Waiting, 20),
        ("failed", Bucket::Attention, Status::Error, 30),
        ("asking-new", Bucket::Attention, Status::Waiting, 70),
    ];
    let order = recent_order(&rows);
    assert_eq!(
        order.need,
        ["asking-new", "asking", "failed", "review", "background"]
    );
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
