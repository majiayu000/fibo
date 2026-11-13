use fibo_core::quota::{Quota, Window};

#[test]
fn severity_boundaries_and_window_order_have_one_owner() {
    let levels: Vec<_> = [49, 50, 79, 80, 81, 94, 95, 100]
        .into_iter()
        .map(|used| {
            Window {
                used,
                ..Default::default()
            }
            .level()
        })
        .collect();
    assert_eq!(
        levels,
        ["low", "low", "low", "mid", "mid", "mid", "high", "high"]
    );
    let q = Quota {
        windows: vec![
            Window {
                minutes: 300,
                used: 80,
                resets_at: 100,
                ..Default::default()
            },
            Window {
                minutes: 10080,
                used: 95,
                resets_at: 50,
                ..Default::default()
            },
            Window {
                minutes: 43200,
                used: 80,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    assert_eq!(
        q.by_usage().iter().map(|w| w.minutes).collect::<Vec<_>>(),
        [10080, 300, 43200]
    );
    assert_eq!(q.tightest(49).unwrap().minutes, 10080);
    assert_eq!(q.tightest(50).unwrap().minutes, 300);
    assert_eq!(q.tightest(100).unwrap().minutes, 43200);
}

#[test]
fn reading_freshness_has_one_deadline_and_survives_old_data() {
    let q: fibo_core::quota::Quota = serde_json::from_str(r#"{"at":1000,"windows":[]}"#).unwrap();
    assert_eq!(q.fresh_until(), 4600);
    assert!(!q.stale(4600));
    assert!(q.stale(4601));
    assert!(!q.stale(999));
}
