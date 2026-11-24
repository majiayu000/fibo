//! How each CLI asks to be trusted with a folder, or to update itself, before any
//! conversation. Every part must be there. Claude Code numbers nothing and starts on 「No, exit」, so Enter alone would quit.
//! Real screens: Claude Code 2.1.318, Codex 0.166 and 0.169 (macOS); Codex's first-run and
//! Windows footers are from its 0.167 source (`tui/src/onboarding/trust_directory.rs`).
use crate::screen_ask::{TrustScreen, UpdateScreen};
use fibo_core::model::Agent;

fn cursor(line: &str) -> bool {
    line.starts_with(['❯', '›', '>'])
}
fn plain(line: &str) -> &str {
    line.trim_start_matches(['❯', '›', '>', ' '])
}
fn option<'a>(lines: &[&'a str], text: &str) -> Option<&'a str> {
    lines.iter().copied().find(|l| plain(l) == text)
}
/// The line under `label`.
fn after(lines: &[&str], label: &str) -> Option<String> {
    let at = lines.iter().position(|l| *l == label)?;
    lines.get(at + 1).map(|l| (*l).to_owned())
}
/// The words from the line opening with `first` up to its menu or a line opening with `stop`.
fn words(lines: &[&str], first: &str, stop: &[&str]) -> Option<String> {
    let at = lines.iter().position(|l| l.starts_with(first))?;
    let end = lines[at..].iter().position(|l| {
        stop.iter().any(|s| l.starts_with(s)) || cursor(l) || plain(l).starts_with("1. ")
    })?;
    Some(lines[at..at + end].join(" "))
}

pub(super) fn claude(lines: &[&str]) -> Option<TrustScreen> {
    if *lines.last()? != "Enter to confirm · Esc to cancel" {
        return None;
    }
    option(lines, "No, exit")?;
    let yes = option(lines, "Yes, I trust this folder")?;
    Some(TrustScreen {
        agent: Agent::Claude,
        question: words(
            lines,
            "Quick safety check:",
            &["Claude Code'll", "Security guide"],
        )?,
        folder: after(lines, "Accessing workspace:")?,
        on_yes: cursor(yes),
        toward_yes: "down",
    })
}

/// What Esc does on Codex's prompt, as its footer ends: back to the Agent Command Center
/// (the daemon's prompt), quit (the first-run one, the only one on Windows), or keep the
/// task's folder (changing folders inside a task).
const CODEX_ESC: [&str; 3] = ["esc back", "esc quit", "esc cancel"];

pub(super) fn codex(lines: &[&str]) -> Option<TrustScreen> {
    // `enter continue · esc back`, or on Windows `enter continue and create sandbox · esc
    // quit`, which a narrow screen wraps onto a second line.
    let at = lines
        .iter()
        .rposition(|l| l.starts_with("enter continue"))?;
    let footer = lines[at..].join(" ");
    if lines.len() - at > 2 || !CODEX_ESC.iter().any(|esc| footer.ends_with(esc)) {
        return None;
    }
    let yes = option(lines, "1. Trust and continue")?;
    Some(TrustScreen {
        agent: Agent::Codex,
        question: words(lines, "Trust this folder?", &[])?,
        folder: after(lines, "Folder access")?,
        on_yes: cursor(yes),
        toward_yes: "up",
    })
}

/// Codex's 「Update available」 picker at start, when a newer release is out: it waits for a
/// key before anything else. Real screens: Codex 0.169 at 100, 40 and 27 columns (11-24);
/// its source is `tui/src/update_prompt.rs`. On a narrow screen every part may wrap: the
/// question right after its 「·」, the command in option 1, option 3 and the footer.
pub(super) fn codex_update(lines: &[&str]) -> Option<UpdateScreen> {
    let at = lines
        .iter()
        .rposition(|l| l.starts_with("enter continue"))?;
    if lines.len() - at > 2 || lines[at..].join(" ") != "enter continue · esc skip" {
        return None;
    }
    let options = &lines[..at];
    let update = options
        .iter()
        .rposition(|l| plain(l).starts_with("1. Update now"))?;
    let choices: Vec<&str> = options[update..].iter().map(|l| plain(l)).collect();
    if !choices
        .join(" ")
        .ends_with(" 2. Skip 3. Skip until next version")
    {
        return None;
    }
    Some(UpdateScreen {
        agent: Agent::Codex,
        question: words(&lines[..update], "Update available ·", &["Release notes:"])?,
    })
}
