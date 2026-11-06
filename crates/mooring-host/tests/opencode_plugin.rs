//! The OpenCode declaration against what a real OpenCode 1.19.5 draws and stores.
use mooring_host::screen_rules::{explain, Rules, Source};
use rusqlite::Connection;
use serde_json::Value;

fn agent() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../plugins/opencode/agent.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}
fn rules() -> &'static Rules {
    let text = agent()["screen_rules"].to_string();
    Box::leak(Box::new(Rules::parse(&text, Source::Bundled).unwrap()))
}

/// Undoing `Side::None` or this rule leaves a session 在跑 while it waits for an answer.
#[test]
fn a_permission_prompt_is_a_wait_and_says_what_it_asks() {
    let asking = "  ┃  △ Permission required\n  ┃    # Shell command\n  ┃\n  ┃  $ sleep 15; echo x\n  ┃\n  ┃\n\
                  ┃   Allow once   Allow always   Reject          ctrl+f fullscreen  ⇆ select  enter confirm\n  ┃\n";
    let seen = explain(rules(), asking);
    assert_eq!(seen.question(), Some("$ sleep 15; echo x"));
    // Not an approval: buttons that answer it press numbers this prompt does not have.
    assert_eq!(seen.approval(), None);
    // An edit shows its diff above the buttons; the question is the file, not a diff line.
    let edit = "  ┃  △ Permission required\n  ┃    → Edit /tmp/a.txt\n  ┃\n  ┃  1 + hi\n  ┃\n\
                ┃   Allow once   Allow always   Reject   ctrl+f fullscreen\n";
    assert_eq!(explain(rules(), edit).question(), Some("→ Edit /tmp/a.txt"));
    let working = "  ┃  Build · DeepSeek V4.1 Flash OpenCode Go · max\n   ⬝⬝⬝⬝⬝⬝⬝⬝  esc interrupt   tab agents  ctrl+p commands\n";
    assert_eq!(explain(rules(), working).question(), None);
    let scrolled = format!("{asking}{}", "output line\n".repeat(6));
    assert_eq!(
        explain(rules(), &scrolled).question(),
        None,
        "answered long ago"
    );
}

/// A turn stopped with Esc or by an API error has no `step-finish`: it only shows in the
/// message. Undoing the UNION leaves the session 在跑 for ever.
#[test]
fn a_turn_that_ended_without_finishing_shows_in_the_records() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE message(id TEXT, session_id TEXT, time_updated INT, data TEXT);
         CREATE TABLE part(id TEXT, message_id TEXT, session_id TEXT, time_updated INT, data TEXT);
         INSERT INTO message VALUES
           ('msg_1','s',1,'{\"role\":\"user\"}'),
           ('msg_2','s',5,'{\"role\":\"assistant\",\"error\":{\"name\":\"MessageAbortedError\"}}'),
           ('msg_3','s',9,'{\"role\":\"assistant\",\"error\":{\"name\":\"APIError\"}}'),
           ('msg_4','s',9,'{\"role\":\"assistant\",\"finish\":\"stop\"}');
         INSERT INTO part VALUES ('prt_1','msg_1','s',2,'{\"type\":\"text\",\"text\":\"hi\"}');",
    )
    .unwrap();
    let plugin = agent();
    let query = plugin["source"]["records"].as_str().unwrap();
    let mut q = db.prepare(query).unwrap();
    let col = q.column_index("type").unwrap();
    let kinds: Vec<String> = q
        .query_map(rusqlite::params!["s", 0, ""], |r| r.get(col))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(kinds, ["text", "message-aborted", "message-error"]);
    let events = plugin["transcripts"]["events"].as_array().unwrap();
    let turn = |kind: &str| {
        events
            .iter()
            .find(|e| e["when"]["/type"] == kind)
            .map(|e| e["turn"].clone())
    };
    assert_eq!(turn("message-aborted"), Some("interrupted".into()));
    assert_eq!(turn("message-error"), Some("error".into()));
    // Rejecting the permission prompt ends the turn with nothing after the tool's error.
    let rejected = events
        .iter()
        .find(|e| e["when"]["/status"] == "error")
        .unwrap();
    assert_eq!(rejected["turn"], "interrupted");
}
