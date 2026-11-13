//! One `fibo events` line: when, what, the whole terminal id, and what it is about.
use super::list_text::one_line;
use fibo_core::hook::Notice;
use fibo_core::reason::fit;

const WHAT: usize = 60;

/// `tab` names the terminal when the notice itself carries no title (a terminal starting).
pub(super) fn line(n: &Notice, tab: &str) -> String {
    let title = if n.title.is_empty() { tab } else { &n.title };
    let what = [title, n.detail.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(one_line)
        .collect::<Vec<_>>()
        .join(" · ");
    let code = n
        .exit_code
        .map(|c| t!(" 退出码 {c}", c))
        .unwrap_or_default();
    let line = format!(
        "{}  {:<16} {:<28}  {}{code}",
        fibo_core::time::clock(n.at),
        n.event,
        n.terminal.as_deref().unwrap_or("-"),
        fit(&what, WHAT),
    );
    line.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn notice(v: serde_json::Value) -> Notice {
        serde_json::from_value(v).unwrap()
    }
    #[test]
    fn the_whole_terminal_id_is_shown() {
        let n = notice(serde_json::json!({
            "event": "terminal_started", "terminal": "21463-1794289462766296000-5"
        }));
        let l = line(&n, "zsh");
        assert!(
            l.contains(" 21463-1794289462766296000-5 ") && l.ends_with("zsh"),
            "{l}"
        );
    }
    #[test]
    fn a_long_detail_stays_on_one_short_line() {
        let n = notice(serde_json::json!({
            "event": "error", "title": "t", "detail": "很长\n的说明".repeat(50)
        }));
        let l = line(&n, "");
        assert!(
            !l.contains('\n') && fibo_core::reason::width(&l) <= 120,
            "{l}"
        );
    }
}
