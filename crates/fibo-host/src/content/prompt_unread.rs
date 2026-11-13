//! A prompt the hook announced that the log never recorded. Esc right after sending puts
//! it back in the input box: Claude Code writes no prompt and no interruption, and no Stop
//! hook fires, so nothing in the log or the hooks ends the turn the hook started.
use super::*;

/// How long such a prompt may go unrecorded before the CLI is asked whether it runs.
/// The log normally records it within the same second.
pub const PROMPT_UNREAD_HOLD: u64 = 10;

/// The turn rests on the prompt hook alone: its log has recorded no prompt for it. Cheap
/// enough for the host to ask on every quiet screen; `Parser::prompt_unread` is exact.
pub fn prompt_unread(c: &Content) -> bool {
    c.turn == Turn::Running
        && c.signal == "Hook"
        && c.signal_detail == "事件：UserPromptSubmit"
        // A prompt the log recorded replaces the hook's `<at>:<offset>` key with its id.
        && c.turn_key.starts_with(&format!("{}:", c.turn_started))
}

impl Parser {
    /// No prompt was read for the running turn since the hook started it.
    pub(super) fn prompt_unread(&self) -> bool {
        prompt_unread(&self.card) && self.turn_id != self.last_user_turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibo_core::model::Agent;

    fn sent(p: &mut Parser, at: u64) {
        p.hook(
            "UserPromptSubmit",
            &HookFacts::default(),
            at,
            Source::default(),
        );
    }

    #[test]
    fn esc_before_the_prompt_is_recorded_ends_the_turn_once_the_cli_is_idle() {
        let s = Source::default();
        let mut p = Parser::default();
        p.add(
            Agent::Claude,
            r#"{"type":"user","uuid":"u1","timestamp":"2026-11-10T11:00:00Z","message":{"content":"推广还差什么"}}"#.as_bytes(),
            s.clone(),
        );
        p.hook("Stop", &HookFacts::default(), 1_794_000_100, s.clone());
        // Sent, then taken back with Esc: only the hook and a mode attachment arrive.
        sent(&mut p, 1_794_480_674);
        p.add(
            Agent::Claude,
            br#"{"type":"attachment","timestamp":"2026-11-12T10:51:14Z","attachment":{"type":"auto_mode"}}"#,
            s.clone(),
        );
        assert_eq!(p.card.turn, Turn::Running);
        assert!(prompt_unread(&p.card) && p.prompt_unread());
        // Busy changes nothing; idle ends it like any other Esc.
        p.official(&Official::Busy, 1_794_480_690, s.clone());
        assert_eq!(p.card.turn, Turn::Running);
        p.official(&Official::Idle, 1_794_480_700, s.clone());
        assert_eq!(p.card.turn, Turn::Interrupted);
        assert_eq!(p.card.signal, "官方状态");
    }

    #[test]
    fn a_prompt_the_log_recorded_is_not_ended_by_an_idle_report() {
        let s = Source::default();
        let mut p = Parser::default();
        sent(&mut p, 1_794_480_674);
        p.add(
            Agent::Claude,
            r#"{"type":"user","uuid":"u2","timestamp":"2026-11-12T10:51:14Z","message":{"content":"再看一下"}}"#.as_bytes(),
            s.clone(),
        );
        assert!(!prompt_unread(&p.card) && !p.prompt_unread());
        // The official state can lag the log: the turn the log shows keeps running.
        p.official(&Official::Idle, 1_794_480_700, s);
        assert_eq!(p.card.turn, Turn::Running);
    }
}
