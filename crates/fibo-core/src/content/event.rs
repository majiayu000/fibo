//! One identity for session notifications, plugin hooks and delegated-turn reports.
use super::Content;
use crate::hook::Event;

impl Content {
    /// One start/finish per turn, one wait per dialog, one error per distinct text.
    /// Refining a waiting prompt or reading more of a final reply is not a new event.
    pub fn event_key(&self, event: Event) -> String {
        match event {
            Event::Waiting => format!(
                "{}:{}:{}:{}",
                event.name(),
                self.turn_key,
                self.turn_started,
                self.waiting_episode
            ),
            Event::Error => format!(
                "{}:{}:{}:{}",
                event.name(),
                self.turn_key,
                self.turn_started,
                self.error
            ),
            Event::Started | Event::Done | Event::CommandDone | Event::CommandFailed => {
                format!("{}:{}:{}", event.name(), self.turn_key, self.turn_started)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_dialog_stays_one_event_when_its_text_is_refined() {
        let mut content = Content {
            turn_key: "turn-1".into(),
            turn_started: 12,
            waiting_episode: 3,
            waiting: "Allow?".into(),
            ..Default::default()
        };
        let key = content.event_key(Event::Waiting);
        assert_eq!(key, "waiting:turn-1:12:3");
        content.waiting = "Allow this command?".into();
        assert_eq!(content.event_key(Event::Waiting), key);
        content.waiting_episode += 1;
        assert_ne!(content.event_key(Event::Waiting), key);
    }

    #[test]
    fn errors_and_turns_keep_their_existing_identity() {
        let mut content = Content {
            turn_key: "turn-1".into(),
            turn_started: 12,
            error: "failed".into(),
            ..Default::default()
        };
        let done = content.event_key(Event::Done);
        assert_eq!(done, "done:turn-1:12");
        assert_eq!(content.event_key(Event::Error), "error:turn-1:12:failed");
        assert_ne!(content.event_key(Event::Started), done);
        content.last_reply = "more of the same reply".into();
        assert_eq!(content.event_key(Event::Done), done);
        let error = content.event_key(Event::Error);
        content.error.push_str(" again");
        assert_ne!(content.event_key(Event::Error), error);
        content.turn_started += 1;
        assert_ne!(content.event_key(Event::Done), done);
    }
}
