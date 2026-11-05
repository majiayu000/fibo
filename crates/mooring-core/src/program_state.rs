//! What a state reported through OSC 7501 means, the one reading of it: for a terminal
//! with no conversation (`status::of_program`) and for a conversation's turn (`apply`).
use crate::content::{Content, Turn};
use crate::protocol::ProgramReport;
use crate::status::Status;

/// The turn a reported state stands for. `idle` (and a word the protocol lacks) is a
/// program that stopped without finishing a turn: 停下了, not 等你验收.
fn turn(state: &str) -> Turn {
    match state {
        "working" => Turn::Running,
        "blocked" => Turn::Waiting,
        "error" => Turn::Error,
        "done" => Turn::Completed,
        _ => Turn::Interrupted,
    }
}

/// A terminal with no conversation whose program reports its own state, read as the
/// state its turn would give a conversation.
pub fn of_program(state: &str) -> Status {
    match turn(state) {
        Turn::Running => Status::Running,
        Turn::Waiting => Status::Waiting,
        Turn::Error => Status::Error,
        Turn::Completed => Status::Review,
        _ => Status::Paused,
    }
}

/// A conversation's turn as its program reported it. `idle` ends no turn: it only stops
/// one left running or asking (a new prompt clears working and blocked, the protocol
/// says) and otherwise leaves what the log says, a background in flight included.
pub fn apply(c: &mut Content, r: &ProgramReport) {
    let turn = turn(&r.state);
    let idle = turn == Turn::Interrupted;
    if idle && !matches!(c.turn, Turn::Running | Turn::Waiting) {
        return;
    }
    let said = if r.msg.is_empty() { &r.title } else { &r.msg };
    if c.turn != turn {
        if turn == Turn::Running {
            c.turn_started = r.since;
        }
        if turn == Turn::Waiting {
            c.waiting = if said.is_empty() {
                "程序说在等你".into()
            } else {
                format!("{said}（程序上报）")
            };
            c.waiting_episode += 1;
        }
        if turn == Turn::Error && c.error.is_empty() {
            c.error = if said.is_empty() {
                "程序上报出错".into()
            } else {
                said.clone()
            };
        }
        c.turn = turn;
    }
    if turn != Turn::Running && !idle {
        c.background = None;
    }
    c.signal = "程序上报".into();
    c.signal_at = r.since;
    c.signal_detail = format!("终端里的程序通过 OSC 7501 报告 {}", r.state);
}

#[cfg(test)]
#[path = "program_state_tests.rs"]
mod tests;
