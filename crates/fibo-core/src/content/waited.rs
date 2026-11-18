//! How long a turn waited on the user, so the clock that times the turn counts only the
//! time the Agent worked: a question answered three hours later is not three hours of work.
use super::{Content, Turn};
use serde::{Deserialize, Serialize};

/// The waits of one turn, added up as each one ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waited {
    /// The `turn_started` these belong to; any other means none.
    pub turn: u64,
    pub secs: u64,
}

impl Content {
    /// Call where the turn's state changed from `before`, while `signal_at` still says
    /// when `before` began: a wait that ends at `at` adds its length to this turn's.
    pub fn end_wait(&mut self, before: Turn, at: u64) {
        if before != Turn::Waiting || self.turn == Turn::Waiting || self.turn_started == 0 {
            return;
        }
        let secs = at.saturating_sub(self.signal_at.max(self.turn_started));
        if self.waited.turn != self.turn_started {
            self.waited = Waited {
                turn: self.turn_started,
                secs: 0,
            };
        }
        self.waited.secs += secs;
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
    fn an_answered_wait_moves_the_clock_by_its_length() {
        let mut c = waiting_since(160);
        c.turn = Turn::Running;
        c.end_wait(Turn::Waiting, 160 + 3 * 3600);
        assert_eq!(c.clock_from(), 100 + 3 * 3600);
        assert_eq!(c.running_since(), Some(100 + 3 * 3600));
    }

    /// Cards an older build saved have no `waited`: the clock reads as it did. One saved
    /// while waiting still knows when its wait began, so answering it after the update
    /// leaves the wait out.
    #[test]
    fn a_card_from_before_counts_from_its_start_until_its_wait_ends() {
        let old: Content =
            serde_json::from_value(serde_json::json!({"turn": "Running", "turn_started": 100,
                "title": "", "asks": [], "evidence": [], "last_reply": "", "turn_key": "",
                "updated": 0, "completed_claim": false, "waiting": "", "error": "",
                "partial_history": false, "from_offset": 0, "unknown_records": 0}))
            .unwrap();
        assert_eq!(old.running_since(), Some(100));
        let mut c = waiting_since(160);
        c.turn = Turn::Running;
        c.end_wait(Turn::Waiting, 760);
        assert_eq!(c.clock_from(), 700);
    }

    #[test]
    fn only_a_wait_that_ends_counts_and_only_for_its_own_turn() {
        let mut c = waiting_since(160);
        c.end_wait(Turn::Running, 500);
        assert_eq!(c.clock_from(), 100, "nothing waited before");
        c.end_wait(Turn::Waiting, 500);
        assert_eq!(c.clock_from(), 100, "still waiting");
        c.turn = Turn::Running;
        c.end_wait(Turn::Waiting, 400);
        c.turn_started = 1000;
        assert_eq!(c.clock_from(), 1000, "a new turn's clock starts afresh");
        // A wait reported from before this turn began counts from its start.
        c.signal_at = 900;
        c.end_wait(Turn::Waiting, 1060);
        assert_eq!(c.clock_from(), 1060);
    }
}
