use super::*;
use crate::agents::modes::menu;

const CLAUDE: &str = include_str!("../../tests/fixtures/mode-screens/claude-effort.txt");
const CLAUDE_SET: &str = include_str!("../../tests/fixtures/mode-screens/claude-effort-set.txt");
const CODEX: &str = include_str!("../../tests/fixtures/mode-screens/codex-effort-current.txt");
const CODEX_OLD: &str = include_str!("../../tests/fixtures/mode-screens/codex-effort.txt");
const CODEX_MORE_MENU: &str =
    include_str!("../../tests/fixtures/mode-screens/codex-effort-more.txt");

/// The slider as it stood with the arrow moved to `level`, columns as Claude Code drew them.
fn slider_at(level: &str) -> String {
    let labels =
        "                        low     medium     high     xhigh      max      Tab to toggle";
    let at = labels.find(level).unwrap() + level.len() / 2;
    let mut bar: Vec<char> = "                        ──────────────────────────────────────────"
        .chars()
        .collect();
    bar[at] = '▲';
    let bar: String = bar.into_iter().collect();
    format!("  Effort\n\n{bar}\n{labels}\n\n  ←/→ to adjust · Enter to confirm · s for this session only · Esc to cancel\n")
}

#[test]
fn claudes_slider_reads_as_the_level_under_its_arrow() {
    assert_eq!(claude_slider(CLAUDE), Some("medium"));
    for level in CLAUDE_LEVELS {
        assert_eq!(claude_slider(&slider_at(level)), Some(level), "{level}");
    }
    // Once it closed, or before it opened, there is no slider.
    assert_eq!(claude_slider(CLAUDE_SET), None);
}

#[test]
fn claude_says_which_level_it_set() {
    assert_eq!(claude_effort_set(CLAUDE_SET).as_deref(), Some("high"));
    assert_eq!(claude_effort_set(CLAUDE), None);
    let hint = CLAUDE.lines().find(|l| l.contains(CLAUDE_HINT)).unwrap();
    assert!(read::offers_session(hint), "{hint}");
}

#[test]
fn codexs_levels_read_without_default_and_with_the_one_on() {
    let m = menu(CODEX, super::super::EFFORT_TITLE).unwrap();
    let o = codex_options_of(&[&m], None);
    let keys: Vec<&str> = o.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, ["Low", "Medium", "High", "Extra high"]);
    assert_eq!(o.iter().find(|o| o.current).unwrap().key, "High");
    assert!(read::offers_session(&m.keys));

    let more = menu(CODEX_MORE_MENU, CODEX_ADVANCED).unwrap();
    let o = codex_options_of(&[&m, &more], None);
    let keys: Vec<&str> = o.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(
        keys,
        ["Low", "Medium", "High", "Extra high", "Max", "Ultra"]
    );
    assert!(read::offers_session(&more.keys));
}

#[test]
fn a_codex_menu_marking_none_takes_the_footers_word() {
    let m = menu(CODEX_OLD, super::super::EFFORT_TITLE).unwrap();
    let o = codex_options_of(&[&m], Some("xhigh"));
    assert_eq!(o.iter().find(|o| o.current).unwrap().key, "Extra high");
    assert_eq!(codex_word("Extra high"), "xhigh");
    assert_eq!(codex_word("Medium"), "medium");
    assert_eq!(codex_level("Medium (default)"), "Medium");
}

#[test]
fn codex_on_ultra_is_still_at_its_prompt() {
    let idle = include_str!("../../tests/fixtures/mode-screens/codex-ultra-idle.txt");
    assert_eq!(
        read::composer(mooring_core::model::Agent::Codex, idle).as_deref(),
        Some("Ask Codex to do anything")
    );
    assert_eq!(codex_footer_level(idle).as_deref(), Some("ultra"));
}
