use super::*;

fn spot(space: &str, focused: &str) -> Spot {
    Spot {
        host: None,
        hosted: None,
        peek: None,
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

#[test]
fn a_session_still_loading_is_noted_once_where_it_lands() {
    // Mid-trip: ⌘J went from a to s2 and the user is still on s2.
    let start = spot("a", "t1");
    let on_s2 = spot("b", "t2");
    let mut trail = Trail::default();
    trail.jump(start.clone(), Some("s1"), "s2");
    // A notification for s3, which has not loaded: nothing is noted while it waits, and
    // the jump is noted when it lands, still from s2, so it goes on with the same trip.
    trail.jump(on_s2.clone(), Some("s2"), "s3");
    assert_eq!(trail.back(spot("c", "t3")), Some(start.clone()));
    // Noted on the click as well, the second note sees s2 while the trail has landed on
    // s3, and the way back to where the trip began is lost.
    let mut twice = Trail::default();
    twice.jump(start, Some("s1"), "s2");
    twice.jump(on_s2.clone(), Some("s2"), "s3");
    twice.jump(on_s2.clone(), Some("s2"), "s3");
    assert_eq!(twice.back(spot("c", "t3")), Some(on_s2));
}

/// From s2, where ⌘J landed, the user opened its to-do's detail over the same pane: the
/// next ⌘J starts a new trip, and the way back is that detail, not where s2's trip began.
#[test]
fn a_page_opened_after_landing_starts_a_new_trip() {
    let start = spot("a", "t1");
    let landed = spot("b", "t2");
    let mut detail = landed.clone();
    detail.page.3 = Some("todo".into());
    let mut trail = Trail::default();
    trail.jump(start.clone(), Some("s1"), "s2");
    trail.landed_on(landed.page.clone());
    trail.jump(detail.clone(), Some("s2"), "s3");
    assert_eq!(trail.back(spot("c", "t3")), Some(detail));
    // Still on the page it landed on, the same trip goes on.
    let mut same = Trail::default();
    same.jump(start.clone(), Some("s1"), "s2");
    same.landed_on(landed.page.clone());
    same.jump(landed, Some("s2"), "s3");
    assert_eq!(same.back(spot("c", "t3")), Some(start));
}
