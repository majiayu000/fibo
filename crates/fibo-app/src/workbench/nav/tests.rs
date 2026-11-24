use super::*;

fn spot(space: &str, focused: &str) -> Spot {
    Spot {
        host: None,
        page: (space.into(), Some(format!("{space}-tab")), false, None),
        focused: Some(focused.into()),
        here: None,
    }
}

#[test]
fn back_returns_to_where_the_jump_left_and_again_to_the_answer() {
    let mut trail = Trail::default();
    assert_eq!(trail.back(spot("a", "t1")), None);
    let start = spot("a", "t1");
    trail.jump(start.clone(), Some("s1"), "s2");
    let answered = spot("b", "t2");
    assert_eq!(trail.back(answered.clone()), Some(start.clone()));
    // Pressed again: back to the session just answered, and so on.
    assert_eq!(trail.back(start.clone()), Some(answered));
    assert_eq!(trail.back(spot("b", "t2")), Some(start));
}

#[test]
fn jumps_in_a_row_keep_where_the_trip_began() {
    let mut trail = Trail::default();
    let start = spot("a", "t1");
    trail.jump(start.clone(), None, "s2");
    trail.jump(spot("b", "t2"), Some("s2"), "s3");
    trail.jump(spot("c", "t3"), Some("s3"), "s4");
    assert_eq!(trail.back(spot("d", "t4")), Some(start));
}

#[test]
fn moving_away_after_a_jump_starts_a_new_trip() {
    let mut trail = Trail::default();
    trail.jump(spot("a", "t1"), None, "s2");
    // The user went to s9 themselves, then pressed ⌘J there.
    let own = spot("c", "t9");
    trail.jump(own.clone(), Some("s9"), "s3");
    assert_eq!(trail.back(spot("b", "t3")), Some(own));
}

#[test]
fn going_back_ends_the_trip() {
    let mut trail = Trail::default();
    let start = spot("a", "t1");
    trail.jump(start.clone(), None, "s2");
    let answered = spot("b", "t2");
    assert_eq!(trail.back(answered.clone()), Some(start.clone()));
    // ⌘J from the spot gone back to is a new trip, even if it is on s2 again.
    trail.jump(start.clone(), Some("s2"), "s3");
    assert_eq!(trail.back(spot("c", "t3")), Some(start));
}

#[test]
fn a_jump_to_what_is_in_view_keeps_the_way_back() {
    let mut trail = Trail::default();
    let start = spot("a", "t1");
    trail.jump(start.clone(), None, "s2");
    // A notification for s2 clicked while already on it.
    trail.jump(spot("b", "t2"), Some("s2"), "s2");
    assert_eq!(trail.back(spot("b", "t2")), Some(start));
    let mut fresh = Trail::default();
    fresh.jump(spot("a", "t1"), Some("s1"), "s1");
    assert_eq!(fresh.back(spot("a", "t1")), None);
}
