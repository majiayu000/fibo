//! What Codex's screen says about background terminals. Its hooks list none, and the state
//! it reports through the terminal is gone after a host restart; the footer stays.

/// The idle footer: 「1 background terminal running · /ps to view · /stop to close」, with no
/// turn ticking. A running turn writes the same words after 「esc to interrupt」.
pub(super) fn left_running(tail: &str) -> bool {
    reports_running(tail) && !bottom(tail).any(|l| l.contains("esc to interrupt"))
}

/// The narrower UI can wrap the controls across multiple rows.
pub(super) fn reports_running(tail: &str) -> bool {
    bottom(tail).any(|l| l.contains("background terminal"))
        && bottom(tail).any(|l| l.contains("/stop to close"))
}

fn bottom(tail: &str) -> impl Iterator<Item = &str> {
    tail.lines().rev().filter(|l| !l.trim().is_empty()).take(6)
}

#[cfg(test)]
mod tests {
    use super::left_running;

    /// The strategy, not the host, says who reads the screen: the host never names a CLI.
    #[test]
    fn only_codex_reads_its_screen_for_background_commands() {
        use crate::agents::{adapter, BackgroundCommands};
        use fibo_core::model::Agent;
        let screen = |a| {
            matches!(
                adapter(a).background_commands(),
                BackgroundCommands::Screen(_)
            )
        };
        assert!(screen(Agent::Codex));
        assert!(!screen(Agent::Claude));
    }
    /// Bottom lines as a real Codex 0.167 drew them, 2026-11-06.
    #[test]
    fn only_a_turn_that_ended_leaves_a_background_terminal() {
        let idle = "  1 background terminal running · /ps to view · /stop to close\n\n\
                    › Ask Codex to do anything\n\n  GPT-6-Astra low · ~/x\n  ← for agents";
        assert!(left_running(idle));
        let working = "• Working (12s • esc to interrupt) · 1 background terminal running · /ps to view · /stop to close\n\n\
                       › Ask Codex to do anything\n\n  GPT-6-Astra high · ~/x\n  ← for agents";
        assert!(!left_running(working));
        assert!(!left_running(
            "› Ask Codex to do anything\n\n  GPT-6-Astra low · ~/x"
        ));
    }
}
