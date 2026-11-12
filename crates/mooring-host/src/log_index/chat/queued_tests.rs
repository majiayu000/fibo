//! A message you send while the Agent works lands between its calls (2026-11-10 walk: ping,
//! then two queued lines, then the reply showed no 「跑了 1 个命令」).
use super::super::Index;

fn index(rows: &[(i64, &str, &str)]) -> (std::path::PathBuf, Index) {
    let dir = std::env::temp_dir().join(format!("mooring-queued-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let i = Index::open(&dir, vec![]).unwrap();
    for (n, (at, kind, text)) in rows.iter().enumerate() {
        i.db.execute(
            "INSERT INTO session_records VALUES('p',1,?1,0,?2,?3,?4,'',NULL)",
            rusqlite::params![n as i64, at, kind, text],
        )
        .unwrap();
    }
    (dir, i)
}

const PING: &str = "Bash {\"command\":\"ping -c 30 127.0.0.1\"}";

#[test]
fn calls_before_a_queued_message_count_on_the_reply() {
    let (dir, i) = index(&[
        (10, "user", "跑一下 ping"),
        (12, "assistant", "好，开始"),
        (13, "tool_call", PING),
        (20, "user", "排队一"),
        (21, "user", "排队二"),
        (50, "assistant", "ping-done q1 q2"),
        (100, "user", "再来"),
        (101, "tool_call", "Bash {\"command\":\"echo hi\"}"),
        (110, "assistant", "hi"),
    ]);
    let (all, _) = i.chat("p", 0, None, None, 50, true).unwrap();
    let said = |at: u64| {
        all.iter()
            .find(|m| m.at == at)
            .unwrap()
            .tools
            .clone()
            .unwrap()
    };
    assert_eq!(said(12).commands, 0);
    assert_eq!((said(50).commands, said(50).took), (1, 37));
    assert_eq!((said(110).commands, said(110).took), (1, 10));
    let reply = all.iter().find(|m| m.at == 50).unwrap().seq;
    // A page that starts on the reply still finds the call and when it began.
    let (page, _) = i.chat("p", 0, None, Some(reply + 1), 1, true).unwrap();
    let t = page[0].tools.clone().unwrap();
    assert_eq!((page[0].at, t.commands, t.took), (50, 1, 37));
    // The list behind the line holds the same call.
    let (tools, rows, _) = i.tool_calls("p", Some(reply), None, 50).unwrap();
    assert_eq!((rows.len(), tools.unwrap().commands), (1, 1));
    std::fs::remove_dir_all(dir).unwrap();
}

/// 2026-11-12 walk: 「停下」 ends a turn with no reply. Counting from the Agent's last message
/// put the stopped ping on the next reply: 「干了 2 分钟 · 跑了 1 个命令」 for 20 seconds
/// that ran no command. The interruption ends the stretch, as a reply would.
#[test]
fn a_stopped_turn_does_not_count_on_the_next_reply() {
    let (dir, i) = index(&[
        (10, "user", "跑一下"),
        (12, "assistant", "ping3-done"),
        (100, "user", "再跑 ping，我会停下"),
        (101, "tool_call", PING),
        (110, "turn", "Interrupted"),
        (200, "user", "写 notes.txt"),
        (201, "tool_call", "Write {\"file_path\":\"notes.txt\"}"),
        (220, "assistant", "diff-ready"),
    ]);
    let (all, _) = i.chat("p", 0, None, None, 50, true).unwrap();
    let reply = all.iter().find(|m| m.at == 220).unwrap();
    let t = reply.tools.clone().unwrap();
    assert_eq!((t.commands, t.took), (0, 20));
    // A page starting on the reply, and the list behind its line, agree.
    let (page, _) = i.chat("p", 0, None, Some(reply.seq + 1), 1, true).unwrap();
    let t = page[0].tools.clone().unwrap();
    assert_eq!((t.commands, t.took), (0, 20));
    let (_, rows, _) = i.tool_calls("p", Some(reply.seq), None, 50).unwrap();
    assert_eq!(rows.len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
