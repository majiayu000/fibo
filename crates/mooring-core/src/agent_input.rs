//! Whether a CLI's input box is on screen, so a task can be handed to it. Most mark the box
//! with an arrow (`terminal::agent_input_ready`); one without declares what its empty box
//! says (`input_hint`), so the main program never spells out a CLI's words.
use crate::agent::Agent;
use crate::terminal::agent_input_ready;

impl Agent {
    /// A declared hint decides alone: an arrow in such a CLI's output is not its input box.
    pub fn input_ready(self, tail: &str) -> bool {
        let hints = self.spec().map_or(&[][..], |s| s.input_hint.as_slice());
        if hints.is_empty() {
            return agent_input_ready(tail);
        }
        let tail = tail.to_lowercase();
        hints.iter().any(|h| tail.contains(&h.to_lowercase()))
    }
}

/// For an Agent started by hand, which nothing here names: any registered CLI's box counts.
pub fn any_input_ready(tail: &str) -> bool {
    agent_input_ready(tail) || Agent::all().into_iter().any(|a| a.input_ready(tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Undoing the arrow fallback stops every built-in CLI from getting its first task.
    #[test]
    fn a_cli_without_a_hint_still_goes_by_its_arrow() {
        let claude = Agent::from_key("Claude");
        assert!(claude.input_ready("❯ \n? for shortcuts"));
        assert!(!claude.input_ready("Ask anything…"));
    }

    #[test]
    fn any_input_ready_keeps_the_arrow_for_an_agent_it_cannot_name() {
        assert!(any_input_ready("› 写一个测试\n? for shortcuts"));
        assert!(!any_input_ready("Do you trust this folder?"));
    }
}
