//! `fibo remote --status` as a person reads it: is the relay up, and for each paired phone
//! whether it is on now, what it watches and what it last did. `--json` keeps the raw form.
use mooring_core::protocol::{RemoteState, RemoteStatus};
use mooring_core::time::ago;
use mooring_host::devices::Role;

/// `now` in Unix seconds.
pub(super) fn render(s: &RemoteStatus, now: u64) -> String {
    let key = if s.keyed { "，有密钥" } else { "" };
    let mut out = match (s.enabled, s.live.state) {
        (false, _) => "中继：没打开（fibo remote --enable --relay <中继地址>）\n".to_owned(),
        (true, RemoteState::Connected) => format!("中继：已连上 {}{key}\n", s.relay),
        (true, RemoteState::Connecting) => format!("中继：正在连 {}{key}\n", s.relay),
        (true, RemoteState::Off) => format!("中继：没在连 {}{key}\n", s.relay),
        (true, RemoteState::Failed) => format!("中继：连不上 {}{key}\n", s.relay),
    };
    if !s.live.detail.is_empty() {
        out.push_str(&format!("  {}\n", s.live.detail));
    }
    if s.devices.is_empty() {
        out.push_str("还没有配对的手机：fibo remote --pair --relay <中继地址> --name <名字>\n");
        return out;
    }
    let online = s.live.online.len();
    out.push_str(&format!(
        "配对的手机 {} 台，在线 {online} 台\n",
        s.devices.len()
    ));
    let name_w = s
        .devices
        .iter()
        .map(|d| mooring_core::reason::width(&d.name))
        .max()
        .unwrap_or(0);
    for d in &s.devices {
        let role = Role::parse(&d.role).map_or_else(|| d.role.clone(), |r| r.label().to_owned());
        let name = format!(
            "{}{}",
            d.name,
            " ".repeat(name_w.saturating_sub(mooring_core::reason::width(&d.name)))
        );
        let phone = s.live.online.iter().find(|p| p.device == d.id);
        let when = match (phone, d.last_seen) {
            (Some(p), _) => format!("在线，{}连上", ago(p.since / 1000, now)),
            (None, Some(at)) => format!("不在线，最近连接 {}", ago(at, now)),
            (None, None) => "没连过".to_owned(),
        };
        let role = format!(
            "{role}{}",
            " ".repeat(8usize.saturating_sub(mooring_core::reason::width(&role)))
        );
        let mark = if phone.is_some() { "●" } else { "○" };
        out.push_str(&format!("{mark} {name}  {role}  {when}  {}\n", d.id));
        if let Some(p) = phone {
            if !p.watching.is_empty() {
                out.push_str(&format!("    在看：{}\n", p.watching.join("、")));
            }
            if let Some(act) = &p.last {
                let on = act
                    .terminal
                    .as_deref()
                    .map_or(String::new(), |t| format!(" {t}"));
                out.push_str(&format!(
                    "    最近操作：{}{on}，{}\n",
                    act.op,
                    ago(act.at / 1000, now)
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn status(v: serde_json::Value) -> RemoteStatus {
        serde_json::from_value(v).unwrap()
    }
    #[test]
    fn times_are_said_as_how_long_ago_not_unix_seconds() {
        let now = 1_794_300_000;
        let out = render(
            &status(serde_json::json!({
                "enabled": true, "relay": "https://relay.example", "keyed": true,
                "devices": [
                    {"id": "2e4ec1dc1f26", "name": "OPPO Find X9 Ultra", "role": "full",
                     "paired": now - 5 * 86_400, "last_seen": now - 7200},
                    {"id": "aa", "name": "iPad", "role": "view", "paired": now, "last_seen": null}
                ],
                "state": "Connected", "detail": "",
                "online": [{"device": "2e4ec1dc1f26", "since": (now - 7200) * 1000,
                            "watching": ["21463-1-5"], "last": null}]
            })),
            now,
        );
        assert!(
            out.starts_with("中继：已连上 https://relay.example，有密钥\n"),
            "{out}"
        );
        assert!(out.contains("配对的手机 2 台，在线 1 台"), "{out}");
        assert!(
            out.contains("● OPPO Find X9 Ultra  完全控制  在线，2 小时前连上"),
            "{out}"
        );
        assert!(out.contains("在看：21463-1-5"), "{out}");
        assert!(out.contains("○ iPad"), "{out}");
        assert!(out.contains("没连过"), "{out}");
        assert!(!out.contains("1794"), "{out}");
    }
    #[test]
    fn off_says_how_to_turn_it_on() {
        let out = render(&RemoteStatus::default(), 0);
        assert!(out.contains("没打开") && out.contains("--enable"), "{out}");
    }
}
