//! A Codex rollout's first read stops at a byte budget, which can fall inside a character.
use super::{codex::Codex, Adapter};

#[test]
fn a_head_cut_inside_a_character_still_names_the_thread() {
    let line = r#"{"type":"session_meta","payload":{"id":"t-1","cwd":"/repo","originator":"codex-tui","base_instructions":{"text":"规则：先读"#;
    // The budget ends one byte into the last character's three.
    let head = &line.as_bytes()[..line.len() - 2];
    assert!(std::str::from_utf8(head).is_err());
    let v = Codex
        .header_value(head)
        .expect("the prefix before the cut is read");
    assert_eq!(v["payload"]["id"], "t-1");
    assert_eq!(v["payload"]["cwd"], "/repo");
    assert_eq!(
        Codex.identity(std::path::Path::new("r.jsonl"), &v),
        Some(("t-1".into(), "/repo".into()))
    );
}
