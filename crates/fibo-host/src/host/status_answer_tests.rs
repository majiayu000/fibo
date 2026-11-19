use super::answered_since;
use std::time::{Duration, Instant};

#[test]
fn a_screen_read_before_the_answer_does_not_bring_the_question_back() {
    let read_at = Instant::now();
    let answered = read_at + Duration::from_millis(5);
    assert!(answered_since(true, Some(answered), read_at));
}

#[test]
fn a_question_drawn_after_the_answer_still_waits() {
    let answered = Instant::now();
    let read_at = answered + Duration::from_millis(5);
    assert!(!answered_since(true, Some(answered), read_at));
    assert!(!answered_since(true, None, read_at));
}

#[test]
fn a_screen_without_a_question_is_never_held_back() {
    let read_at = Instant::now();
    assert!(!answered_since(
        false,
        Some(read_at + Duration::from_millis(5)),
        read_at
    ));
}
