//! Asking a CLI's own local HTTP API what it is waiting on, and answering it
//! (`mooring_core::agent_answers`). Only ever 127.0.0.1, on a port its own process listens
//! on. Plain HTTP/1.1 over a socket: two small requests are not worth a client library.
use base64::{engine::general_purpose::STANDARD, Engine};
use mooring_core::agent_answers::Answers;
use serde_json::Value;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(3);

/// What the CLI is asking, as its API says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub id: String,
    pub question: String,
    pub kind: String,
    /// What 「always」 grants, in words; empty when it has no such option.
    pub always: String,
}

/// Where a CLI's API is: the port its process listens on, and its credentials.
#[derive(Clone, Debug)]
pub struct Endpoint {
    port: u16,
    /// The `Authorization` header's value. Never logged.
    auth: Option<String>,
}
/// `None` when the process opened no port.
pub fn endpoint(api: &Answers, pid: u32) -> Option<Endpoint> {
    let port = listening_port(pid)?;
    let auth = api.password_env.as_deref().and_then(|name| {
        let password = mooring_platform::inspect::env_var(pid, name)?;
        let user = api.username.as_deref().unwrap_or_default();
        Some(format!(
            "Basic {}",
            STANDARD.encode(format!("{user}:{password}"))
        ))
    });
    Some(Endpoint { port, auth })
}
/// The TCP port a process listens on on this machine, if any.
fn listening_port(pid: u32) -> Option<u16> {
    crate::recognition::LSOF_SPAWNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let found = mooring_platform::inspect::tcp_sockets(&[pid]).ok()?;
    found.listening.first().map(|(_, port)| *port)
}
/// The first thing it is asking, or `None` when it asks nothing.
pub fn first_asked(api: &Answers, at: &Endpoint) -> io::Result<Option<Asked>> {
    let list = call(at, "GET", &api.pending, None)?;
    Ok(list
        .as_array()
        .and_then(|l| l.first())
        .and_then(|entry| read(api, entry)))
}
fn read(api: &Answers, entry: &Value) -> Option<Asked> {
    let text = |p: &str| {
        entry
            .pointer(p)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let always: Vec<&str> = api
        .always
        .as_deref()
        .and_then(|p| entry.pointer(p))
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    Some(Asked {
        id: text(&api.id)?.to_owned(),
        question: api.subject.iter().find_map(|p| text(p))?.to_owned(),
        kind: api
            .kind
            .as_deref()
            .and_then(text)
            .unwrap_or_default()
            .to_owned(),
        always: if always.is_empty() {
            String::new()
        } else {
            format!("以后都允许 {}", always.join("、"))
        },
    })
}
/// Answers `id`: allow, allow always, or refuse.
pub fn reply(api: &Answers, at: &Endpoint, id: &str, allow: bool, always: bool) -> io::Result<()> {
    let body = match (allow, always) {
        (true, true) => &api.allow_always,
        (true, false) => &api.allow,
        (false, _) => &api.deny,
    };
    call(at, "POST", &api.reply.replace("{id}", id), Some(body)).map(|_| ())
}
fn call(at: &Endpoint, method: &str, path: &str, body: Option<&Value>) -> io::Result<Value> {
    let port = at.port;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let body = body.map_or_else(String::new, Value::to_string);
    // One write: a server may read the request in one go, and a socket sends what it is given.
    let auth = at
        .auth
        .as_deref()
        .map_or_else(String::new, |a| format!("Authorization: {a}\r\n"));
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Accept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes())?;
    let mut raw = Vec::new();
    stream.take(1 << 22).read_to_end(&mut raw)?;
    parse_response(&raw)
}
fn parse_response(raw: &[u8]) -> io::Result<Value> {
    let bad = |what: &str| io::Error::other(format!("CLI 的接口回复不对：{what}"));
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| bad("没有头"))?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status = head.split_whitespace().nth(1).unwrap_or_default();
    if !status.starts_with('2') {
        return Err(bad(&format!("状态 {status}")));
    }
    let chunked = head.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    let rest = &raw[split + 4..];
    let body = if chunked {
        unchunk(rest).ok_or_else(|| bad("分块"))?
    } else {
        rest.to_vec()
    };
    if body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&body).map_err(|e| bad(&e.to_string()))
}
fn unchunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let end = rest.windows(2).position(|w| w == b"\r\n")?;
        let size =
            usize::from_str_radix(std::str::from_utf8(&rest[..end]).ok()?.trim(), 16).ok()?;
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(rest.get(end + 2..end + 2 + size)?);
        rest = rest.get(end + 4 + size..)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::net::TcpListener;

    fn api() -> Answers {
        serde_json::from_value(json!({
            "pending": "/permission", "id": "/id",
            "subject": ["/metadata/command", "/metadata/filepath", "/permission"],
            "kind": "/permission", "always": "/always",
            "reply": "/permission/{id}/reply",
            "allow": {"reply": "once"}, "allow_always": {"reply": "always"}, "deny": {"reply": "reject"},
        }))
        .unwrap()
    }
    /// A one-request server; what it was sent comes back through the handle.
    fn serve(reply: &'static str) -> (u16, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            let n = s.read(&mut buf).unwrap();
            s.write_all(reply.as_bytes()).unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (port, handle)
    }
    /// Shaped as OpenCode 1.19.5's `/permission` answered, 2026-11-07.
    #[test]
    fn what_a_cli_asks_comes_from_the_declared_fields() {
        let body = r#"[{"id":"per_1","permission":"bash","metadata":{"command":"sleep 3"},"always":["sleep *","echo *"]}]"#;
        let reply: &'static str = Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .into_boxed_str(),
        );
        let (port, sent) = serve(reply);
        let asked = first_asked(&api(), &Endpoint { port, auth: None })
            .unwrap()
            .unwrap();
        assert_eq!(asked.id, "per_1");
        assert_eq!(asked.question, "sleep 3");
        assert_eq!(asked.kind, "bash");
        assert_eq!(asked.always, "以后都允许 sleep *、echo *");
        assert!(sent.join().unwrap().starts_with("GET /permission HTTP/1.1"));
    }
    #[test]
    fn nothing_asked_is_none_and_a_chunked_reply_reads() {
        let (port, _) =
            serve("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n[]\r\n0\r\n\r\n");
        assert_eq!(
            first_asked(&api(), &Endpoint { port, auth: None }).unwrap(),
            None
        );
    }
    /// Refusing must not be sent as allowing, whatever else is passed.
    #[test]
    fn each_answer_posts_its_own_body() {
        for (allow, always, want) in [
            (true, false, r#"{"reply":"once"}"#),
            (true, true, r#"{"reply":"always"}"#),
            (false, true, r#"{"reply":"reject"}"#),
        ] {
            let (port, sent) = serve("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ntrue");
            reply(
                &api(),
                &Endpoint { port, auth: None },
                "per_1",
                allow,
                always,
            )
            .unwrap();
            let sent = sent.join().unwrap();
            assert!(
                sent.starts_with("POST /permission/per_1/reply HTTP/1.1"),
                "{sent}"
            );
            assert!(sent.ends_with(want), "{sent}");
        }
    }
    /// The password goes in the header, and only there.
    #[test]
    fn credentials_go_in_the_header() {
        let (port, sent) = serve("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n[]");
        let at = Endpoint {
            port,
            auth: Some("Basic b3BlbmNvZGU6cA==".into()),
        };
        first_asked(&api(), &at).unwrap();
        assert!(sent
            .join()
            .unwrap()
            .contains("Authorization: Basic b3BlbmNvZGU6cA==\r\n"));
    }
    #[test]
    fn a_refusal_from_the_api_is_an_error() {
        let (port, _) = serve("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        assert!(reply(&api(), &Endpoint { port, auth: None }, "gone", true, false).is_err());
    }
}
