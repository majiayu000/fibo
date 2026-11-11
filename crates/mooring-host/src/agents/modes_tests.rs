use super::*;

const CLAUDE_MODEL: &str = include_str!("../../tests/fixtures/mode-screens/claude-model.txt");
const CODEX_MODEL: &str = include_str!("../../tests/fixtures/mode-screens/codex-model.txt");
const CODEX_EFFORT: &str = include_str!("../../tests/fixtures/mode-screens/codex-effort.txt");
const CODEX_PERMISSIONS: &str =
    include_str!("../../tests/fixtures/mode-screens/codex-permissions.txt");
const CODEX_CONFIRM: &str =
    include_str!("../../tests/fixtures/mode-screens/codex-full-confirm.txt");
const CODEX_IDLE: &str = include_str!("../../tests/fixtures/mode-screens/codex-idle.txt");

fn labels(m: &Menu) -> Vec<&str> {
    m.rows.iter().map(|r| r.label.as_str()).collect()
}

#[test]
fn claudes_model_menu_reads_as_its_rows_with_the_one_in_use() {
    let m = menu(CLAUDE_MODEL, "Select model").unwrap();
    assert_eq!(
        labels(&m),
        [
            "Default (recommended)",
            "Opus 5.5",
            "Fable 5.1",
            "Sonnet 5.5",
            "Haiku 5.5",
            "More models"
        ]
    );
    assert!(m.rows[1].current);
    assert!(m.rows[1].pointer);
    assert_eq!(pointed(&m).unwrap().label, "Opus 5.5");
    assert!(m.rows.iter().filter(|r| r.current).count() == 1);
    assert_eq!(m.rows[3].detail, "Most efficient for simpler tasks");
    assert!(offers_session(&m.keys), "{}", m.keys);
}

#[test]
fn codexs_menus_read_the_same_way() {
    let m = menu(CODEX_MODEL, "Select Model and Effort").unwrap();
    assert_eq!(m.rows[0].label, "GPT-6.1-Sol");
    assert!(m.rows[0].current);
    assert_eq!(m.rows.len(), 8);
    assert!(m.more, "the arrow under the rows says more are below");
    assert!(!offers_session(&m.keys));

    let e = menu(CODEX_EFFORT, "Select Reasoning Level").unwrap();
    assert_eq!(pointed(&e).unwrap().label, "Medium (default)");
    assert!(offers_session(&e.keys), "{}", e.keys);

    let p = menu(CODEX_PERMISSIONS, "Update Model Permissions").unwrap();
    assert_eq!(
        labels(&p),
        [
            "Ask for approval",
            "Approve for me",
            "Full Access",
            "Read Only"
        ]
    );
    assert_eq!(
        p.rows.iter().find(|r| r.current).unwrap().label,
        "Full Access"
    );

    let c = menu(CODEX_CONFIRM, "Enable full access?").unwrap();
    assert_eq!(pointed(&c).unwrap().label, "Yes, continue anyway");
}

#[test]
fn a_menu_is_found_only_while_it_stands() {
    assert!(menu(CODEX_IDLE, "Update Model Permissions").is_none());
    assert!(menu(CLAUDE_MODEL, "Select Model and Effort").is_none());
    // Its title with no hint line under it yet is no menu to press keys at.
    assert!(menu("Select model\n  1. One  x\n", "Select model").is_none());
}

#[test]
fn the_box_is_read_only_when_nothing_stands_over_it() {
    assert_eq!(
        composer(Agent::Codex, CODEX_IDLE).as_deref(),
        Some("Ask Codex to do anything")
    );
    assert_eq!(composer(Agent::Codex, CODEX_PERMISSIONS), None);
    assert!(
        composer(Agent::Codex, CODEX_CONFIRM).is_some(),
        "under the dialog"
    );
    assert_eq!(
        composer(Agent::Claude, CLAUDE_MODEL),
        None,
        "its menu is over the box"
    );
    let claude = "❯ /model\n────\n  ⏸ manual mode on\n";
    assert_eq!(composer(Agent::Claude, claude).as_deref(), Some("/model"));
    let draft = "❯ half a thought/model\n────\n";
    assert_eq!(
        composer(Agent::Claude, draft).as_deref(),
        Some("half a thought/model")
    );
    assert_eq!(composer(Agent::Claude, "no prompt\n"), None);
}

#[test]
fn a_dialog_with_its_hint_below_the_pointer_is_no_input_box() {
    let trust =
        "❯ 1. Yes, I trust this folder\n  2. No, exit\n\n Enter to confirm · Esc to cancel\n";
    assert_eq!(composer(Agent::Claude, trust), None);
    let approve = "› 1. Yes, proceed (y)\n  2. No (esc)\n  enter select · esc back\n";
    assert_eq!(composer(Agent::Codex, approve), None);
}

#[test]
fn footers_say_the_mode_and_the_model() {
    let s = |foot: &str| format!("❯\n─────\n  🪞🔴🔴🟢\n  {foot}\n");
    assert_eq!(
        claude_mode(&s("⏵⏵ auto mode on (shift+tab to cycle)")),
        Some("auto")
    );
    assert_eq!(claude_mode(&s("⏸ manual mode on")), Some("manual"));
    assert_eq!(
        claude_mode(&s("⏵⏵ accept edits on (shift+tab to cycle)")),
        Some("acceptEdits")
    );
    assert_eq!(
        claude_mode(&s("⏸ plan mode on (shift+tab to cycle)")),
        Some("plan")
    );
    assert_eq!(
        claude_mode(&s("⏵⏵ bypass permissions on (shift+tab to cycle)")),
        Some("bypassPermissions")
    );
    // An older version writes nothing on manual: the box is there and no mode is named.
    assert_eq!(claude_mode("❯\n─────\n  ? for shortcuts\n"), Some("manual"));
    assert_eq!(claude_mode("no prompt on this screen\n"), None);
    assert_eq!(
        codex_footer(CODEX_IDLE).as_deref(),
        Some("GPT-6-Sol medium")
    );
    assert!(
        codex_footer(CODEX_PERMISSIONS).is_none()
            || menu(CODEX_PERMISSIONS, "Update Model Permissions").is_some()
    );
}

#[test]
fn claude_says_what_it_set() {
    let t = "❯ /model\n  ⎿  Set model to Sonnet 5.5 for this session only\n";
    assert_eq!(claude_model_set(t).as_deref(), Some("Sonnet 5.5"));
    assert_eq!(
        claude_model_set("❯ /model\n  ⎿  Kept model as Opus 5.5\n"),
        None
    );
}

#[test]
fn claude_asks_before_it_leaves_a_cached_model() {
    let t = include_str!("../../tests/fixtures/mode-screens/claude-switch-confirm.txt");
    assert!(claude_switch_question(t, "Opus 5.5"));
    assert!(
        !claude_switch_question(t, "Sonnet 5.5"),
        "another model than was asked for"
    );
    assert!(!claude_switch_question(CLAUDE_MODEL, "Opus 5.5"));
}

#[test]
fn only_its_own_flags_put_the_bypass_mode_in_the_cycle() {
    assert!(claude_offers_full("claude --dangerously-skip-permissions"));
    assert!(claude_offers_full(
        "/x/claude --allow-dangerously-skip-permissions --resume a"
    ));
    assert!(claude_offers_full(
        "claude --permission-mode bypassPermissions"
    ));
    assert!(!claude_offers_full("claude"));
    assert!(!claude_offers_full("claude --permission-mode plan"));
    assert!(!claude_offers_full("vim dangerously-skip-permissions.md"));
}

#[test]
fn what_lies_on_the_way_to_a_mode() {
    assert!(claude_crosses_full("plan", "auto"));
    assert!(claude_crosses_full("manual", "auto"));
    assert!(claude_crosses_full("acceptEdits", "auto"));
    assert!(!claude_crosses_full("auto", "manual"));
    assert!(!claude_crosses_full("manual", "plan"));
    assert!(!claude_crosses_full("auto", "plan"));
    assert!(!claude_crosses_full("plan", "bypassPermissions"));
    assert!(!claude_crosses_full("bypassPermissions", "plan"));
    assert!(!claude_crosses_full("manual", "manual"));
}

#[test]
fn an_unknown_codex_row_counts_as_full_until_read() {
    assert!(codex_is_full("Full Access"));
    assert!(codex_is_full("Danger zone"));
    assert!(!codex_is_full("Ask for approval"));
    assert!(!codex_is_full("Approve for me"));
    assert!(!codex_is_full("Read Only"));
}

#[test]
fn a_detail_wrapped_by_a_narrow_terminal_reads_whole() {
    let narrow = include_str!("../../tests/fixtures/mode-screens/claude-model-narrow.txt");
    let m = menu(narrow, "Select model").unwrap();
    assert_eq!(
        labels(&m),
        ["Default (recommended)", "Opus", "Fable", "Sonnet", "Haiku"]
    );
    assert_eq!(
        m.rows[1].detail,
        "Opus 5.5 · Best for everyday, complex tasks · $4/$20 per Mtok"
    );
    assert!(m.rows[1].current);
    assert_eq!(
        m.rows[2].detail,
        "Fable 5.1 · Most capable for your hardest and longest-running tasks"
    );
    // The effort line under the rows is not a row's detail.
    assert!(
        m.rows[4].detail.ends_with("for prompts over 100k)"),
        "{}",
        m.rows[4].detail
    );
    assert!(offers_session(&m.keys), "{}", m.keys);
}

#[test]
fn claude_log_and_menu_share_permission_words() {
    for mode in [
        "default",
        "acceptEdits",
        "plan",
        "auto",
        "dontAsk",
        "bypassPermissions",
        "future",
    ] {
        assert_eq!(
            card_words(Agent::Claude, mode),
            super::super::permission::claude(mode),
            "{mode}"
        );
    }
    assert_eq!(
        card_words(Agent::Claude, "manual"),
        card_words(Agent::Claude, "default")
    );
}
