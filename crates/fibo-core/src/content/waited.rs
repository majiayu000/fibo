//! How long a turn waited on the user, so the clock that times the turn counts only the
//! time the Agent worked: a question answered three hours later is not three hours of work.
//!
//! A wait ends when the user answered, which the host sees as input to the terminal (the
//! window, the phone, `fibo send`). The CLI's next signal comes later: after an approved
//! tool has run, which can be minutes of work. Without a known answer nothing is left out,
//! as before 11-18 (docs/decisions/2026-11-19-turn-clock-waits.md).
use super::{Content, Turn};
use serde::{Deserialize, Serialize};

/// The waits of one turn, added up as each one ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waited {
    /// The `turn_started` these belong to; any other means none.
    pub turn: u64,
    pub secs: u64,
    /// When the user's input first answered the current or last wait; 0 unknown. Only an
    /// answer within the wait counts, so one from an earlier wait is ignored.
    #[serde(default)]
    pub answered: u64,
}

impl Content {
    /// When the current or last wait began.
    fn wait_from(&self) -> u64 {
        self.signal_at.max(self.turn_started)
    }
    /// Whether this card waits on the user and they already answered it.
    pub fn answered(&self) -> bool {
        self.turn == Turn::Waiting
            && self.waited.answered != 0
            && self.waited.answered >= self.wait_from()
    }
    /// Waits on the user, who has not answered yet: the next input is the answer.
    pub fn waits_for_answer(&self) -> bool {
        self.turn == Turn::Waiting && !self.answered()
    }
    /// The user's input answered what this card waits on, at `at`. The first input counts:
    /// an arrow key to pick an option is the user already there.
    pub fn answer(&mut self, at: u64) {
        if self.waits_for_answer() {
            self.waited.answered = at;
        }
    }
    /// Call where the turn's state changed from `before`, while `signal_at` still says
    /// when `before` began: a wait that ends at `at` and was answered within it adds the
    /// time until that answer to this turn's.
    pub fn end_wait(&mut self, before: Turn, at: u64) {
        if before != Turn::Waiting || self.turn == Turn::Waiting || self.turn_started == 0 {
            return;
        }
        let from = self.wait_from();
        let answered = self.waited.answered;
        if answered == 0 || answered < from || answered > at {
            return;
        }
        if self.waited.turn != self.turn_started {
            self.waited = Waited {
                turn: self.turn_started,
                secs: 0,
                answered,
            };
        }
        self.waited.secs += answered - from;
    }
    /// Where this turn's clock starts: its start moved on by the time it waited on the
    /// user. 0 when the turn's start is not known.
    pub fn clock_from(&self) -> u64 {
        if self.turn_started == 0 {
            return 0;
        }
        let waited = (self.waited.turn == self.turn_started).then_some(self.waited.secs);
        self.turn_started + waited.unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting_since(at: u64) -> Content {
        Content {
            turn: Turn::Waiting,
            turn_started: 100,
            signal_at: at,
            ..Default::default()
        }
    }

    #[test]
    fn a_wait_counts_until_the_user_answered_not_until_the_cli_said_so() {
        let mut c = waiting_since(160);
        c.answer(160 + 3 * 3600);
        assert!(c.answered());
        // An approved tool ran ten more minutes before its PostToolUse: that was work.
        c.turn = Turn::Running;
        c.end_wait(Turn::Waiting, 160 + 3 * 3600 + 600);
        assert_eq!(c.clock_from(), 100 + 3 * 3600);
        assert_eq!(c.running_since(), Some(100 + 3 * 3600));
    }

    #[test]
    fn the_first_input_answers_and_later_keys_do_not_move_it() {
        let mut c = waiting_since(160);
        c.answer(200);
        c.answer(260);
        assert_eq!(c.waited.answered, 200);
    }

    /// Outside Fibo's terminals nobody sees the answer: nothing is left out, as before.
    #[test]
    fn a_wait_with_no_answer_seen_leaves_the_clock_alone() {
        let mut c = waiting_since(160);
        c.turn = Turn::Running;
        c.end_wait(Turn::Waiting, 760);
        assert_eq!(c.clock_from(), 100);
    }

    /// Cards an older build saved have no `waited`: the clock reads as it did.
    #[test]
    fn a_card_from_before_counts_from_its_start() {
        let old: Content =
            serde_json::from_value(serde_json::json!({"turn": "Running", "turn_started": 100,
                "title": "", "asks": [], "evidence": [], "last_reply": "", "turn_key": "",
                "updated": 0, "completed_claim": false, "waiting": "", "error": "",
                "partial_history": false, "from_offset": 0, "unknown_records": 0}))
            .unwrap();
        assert_eq!(old.running_since(), Some(100));
        // One saved by #427's first build, with `waited` but no `answered`.
        let mid: Waited =
            serde_json::from_value(serde_json::json!({"turn": 100, "secs": 60})).unwrap();
        assert_eq!(mid.answered, 0);
    }

    #[test]
    fn only_an_answer_within_the_wait_counts_and_only_for_its_own_turn() {
        let mut c = waiting_since(160);
        c.answer(150);
        assert_eq!(c.waited.answered, 150, "recorded, though before this wait");
        c.turn = Turn::Running;
        c.end_wait(Turn::Waiting, 500);
        assert_eq!(
            c.clock_from(),
            100,
            "an answer from before the wait is not this one"
        );
        c.turn = Turn::Waiting;
        c.signal_at = 600;
        c.answer(700);
        c.turn = Turn::Running;
        c.end_wait(Turn::Waiting, 900);
        assert_eq!(c.clock_from(), 200);
        c.turn_started = 1000;
        assert_eq!(c.clock_from(), 1000, "a new turn's clock starts afresh");
    }
}
