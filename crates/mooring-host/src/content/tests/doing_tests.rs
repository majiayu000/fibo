//! What a running turn says it is doing while one command runs for minutes.
use super::*;

fn claude(p: &mut Parser, n: u64, v: Value) {
    let source = Source {
        path: "fixture".into(),
        inode: 1,
        offset: n * 1024,
    };
    p.add(Agent::Claude, &serde_json::to_vec(&v).unwrap(), source);
}
fn bash(p: &mut Parser, n: u64, id: &str, command: &str, at: &str) {
    claude(
        p,
        n,
        json!({"type":"assistant","timestamp":at,"message":{"model":"claude","content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":command}}]}}),
    );
}
fn done(p: &mut Parser, n: u64, id: &str, at: &str) {
    claude(
        p,
        n,
        json!({"type":"user","timestamp":at,"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}}),
    );
}

#[test]
fn a_long_command_names_itself_while_it_runs_not_the_one_before() {
    let mut p = Parser::default();
    claude(
        &mut p,
        0,
        json!({"type":"user","timestamp":"2026-11-11T22:06:00Z","message":{"content":"run ls, then ping"}}),
    );
    bash(&mut p, 1, "a", "ls", "2026-11-11T22:06:01Z");
    done(&mut p, 2, "a", "2026-11-11T22:06:02Z");
    assert_eq!(p.card.doing().as_deref(), Some("运行 ls"));
    bash(
        &mut p,
        3,
        "b",
        "ping -c 60 127.0.0.1",
        "2026-11-11T22:06:03Z",
    );
    // Seen on the phone 2026-11-11: fifty seconds into the ping it still said 「正在运行 ls」.
    assert_eq!(p.card.doing().as_deref(), Some("运行 ping -c 60 127.0.0.1"));
    done(&mut p, 4, "b", "2026-11-11T22:07:03Z");
    assert_eq!(p.card.running_command, None);
    assert_eq!(p.card.doing().as_deref(), Some("运行 ping -c 60 127.0.0.1"));
}
