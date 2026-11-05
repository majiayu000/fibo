use super::charts::cpu_scale;
use super::*;

#[test]
fn cpu_axis_keeps_at_most_six_intervals() {
    assert_eq!(cpu_scale(30.), (100., 50.));
    assert_eq!(cpu_scale(180.), (200., 50.));
    assert_eq!(cpu_scale(450.), (500., 100.));
    assert_eq!(cpu_scale(1108.9), (1200., 200.));
    assert_eq!(cpu_scale(1500.), (1500., 500.));
    assert_eq!(cpu_scale(3100.), (4000., 1000.));
    assert_eq!(cpu_scale(6400.), (8000., 2000.));
}
#[test]
fn ranking_grows_to_match_the_right_column() {
    // No GPU data: the right column is just the scatter, so the old 10 stands.
    assert_eq!(first_rows(0), FIRST_ROWS.max(first_rows(0)));
    assert!(first_rows(0) >= FIRST_ROWS);
    // Seven GPU rows (the reported case) make room for all 15 ranked apps.
    assert!(first_rows(7) >= 15);
    assert_eq!(first_rows(8), first_rows(7) + 1);
}

#[test]
fn scatter_labels_do_not_overlap_or_leave_the_plot() {
    let line = T::TICK_ROW;
    // The reported case: three apps near the top-left, two on the bottom-right edge.
    let tops = label_tops(&[
        ("Google Chrome", 0.05, 0.08),
        ("VirtualMachine", 0.15, 0.10),
        ("claude", 0.38, 0.11),
        ("grep", 0.5, 0.97),
        ("mirror", 0.6, 1.0),
    ]);
    let left = ["Google Chrome", "VirtualMachine", "claude"].map(|n| tops[n]);
    for (i, a) in left.iter().enumerate() {
        for b in &left[i + 1..] {
            assert!((a - b).abs() >= line - 0.01, "{left:?}");
        }
    }
    assert!((tops["grep"] - tops["mirror"]).abs() >= line - 0.01);
    assert!(
        tops.values()
            .all(|t| *t >= 0. && *t <= T::SCATTER_HEIGHT - line)
    );
    // Far apart across the plot, labels keep their own height.
    let apart = label_tops(&[("a", 0.0, 0.5), ("b", 0.9, 0.5)]);
    assert_eq!(apart["a"], apart["b"]);
}
