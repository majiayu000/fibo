//! How each CLI asks to be trusted with a folder, before any conversation. Every part must
//! be there. Claude Code numbers nothing and starts on 「No, exit」, so Enter alone would quit.
//! Real screens: Claude Code 2.1.318, Codex 0.166.
use crate::screen_ask::TrustScreen;
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

pub(super) fn codex(lines: &[&str]) -> Option<TrustScreen> {
    let footer = lines.last()?;
    if !(footer.starts_with("enter continue") && footer.contains("esc back")) {
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
