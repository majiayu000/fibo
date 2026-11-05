use super::*;

#[test]
fn only_moves_that_are_actions_drop() {
    assert!(legal(Column::Todo, Column::Running));
    assert!(legal(Column::Review, Column::Done));
    assert!(legal(Column::Needs, Column::Done));
    assert!(legal(Column::Done, Column::Needs));
    for from in [Column::Running, Column::Stale] {
        for to in [
            Column::Todo,
            Column::Running,
            Column::Needs,
            Column::Review,
            Column::Done,
        ] {
            assert!(!legal(from, to), "{from:?} → {to:?}");
        }
    }
    assert!(!legal(Column::Todo, Column::Done));
}

#[test]
fn a_refused_drop_says_why() {
    assert!(refusal(Column::Needs).contains("做完了"));
    assert!(refusal(Column::Running).contains("Agent"));
}

#[test]
fn the_spring_settles_where_it_aims() {
    assert!(spring(0.).abs() < 1e-6);
    assert!((spring(1.) - 1.).abs() < 1e-6);
    assert!(
        (0..=20).any(|i| spring(i as f32 / 20.) > 1.),
        "it overshoots"
    );
}
