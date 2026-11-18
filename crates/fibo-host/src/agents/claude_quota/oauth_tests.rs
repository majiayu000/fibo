use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

fn reply(status: &str, headers: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/usage", server.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
        body.len()
    );
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = server.accept().unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        socket.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    });
    (url, worker)
}

#[test]
fn real_http_transport_reads_the_response_without_launching_a_cli() {
    let (url, server) = reply(
        "200 OK",
        "",
        r#"{"five_hour":{"utilization":19,"resets_at":"2026-11-16T12:00:00Z"},"seven_day":{"utilization":37,"resets_at":null}}"#,
    );
    let got = request(
        &url,
        &Credential::new("test-credential".into()).unwrap(),
        1000,
    )
    .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(got["session"]["used"], 19.0);
    assert_eq!(got["session"]["resets_at"], 1794830400u64);
    assert_eq!(got["week"]["used"], 37.0);
    let sent = server.join().unwrap().to_ascii_lowercase();
    assert!(sent.contains("authorization: bearer test-credential"));
    assert!(sent.contains("anthropic-beta: oauth-2025-04-20"));
}

#[test]
fn server_errors_never_echo_credentials_or_response_bodies() {
    for status in [
        "302 Found",
        "401 Unauthorized",
        "429 Too Many Requests",
        "503 Unavailable",
    ] {
        let (url, server) = reply(
            status,
            "Retry-After: 321\r\nLocation: http://127.0.0.1:1/leak\r\n",
            "secret echoed by upstream",
        );
        let err = request(&url, &Credential::new("secret".into()).unwrap(), 1000)
            .err()
            .unwrap();
        assert!(!err.message.contains("secret"));
        if status.starts_with("429") {
            assert_eq!(err.retry_at, Some(1321));
        }
        if status.starts_with("401") {
            assert!(err.discard);
        }
        server.join().unwrap();
    }
}

#[test]
fn missing_and_invalid_windows_are_not_zero_usage() {
    for doc in [
        json!({}),
        json!({"error":"unavailable"}),
        json!({"five_hour":{"resets_at":null}}),
        json!({"five_hour":{"utilization":19,"resets_at":"broken"}}),
    ] {
        assert!(windows(&doc).is_err(), "{doc}");
    }
    let empty = windows(&json!({"five_hour":null,"seven_day":null})).unwrap();
    assert!(empty.get("session").is_none());
    assert!(empty.get("week").is_none());
}

#[test]
fn scoped_model_windows_keep_their_names_and_do_not_duplicate_old_fields() {
    let got=windows(&json!({"five_hour":null,"seven_day":null,
        "seven_day_sonnet":{"utilization":15,"resets_at":null},
        "limits":[{"group":"weekly","kind":"weekly_scoped","percent":31,"resets_at":null,"scope":{"model":{"display_name":"Fable"}}}]})).unwrap();
    assert_eq!(got["models"][0]["name"], "sonnet");
    assert_eq!(got["models"][1]["name"], "Fable");
    assert_eq!(got["models"][1]["used"], 31.0);
}

#[test]
fn retry_after_handles_seconds_dates_and_missing_headers() {
    assert_eq!(retry_at(Some("321"), 1000), 1321);
    assert_eq!(retry_at(Some("Thu, 01 Jan 1970 00:30:00 GMT"), 1000), 1800);
    assert_eq!(retry_at(None, 1000), 1300);
    assert_eq!(retry_at(Some("0"), 1000), 1060);
}

#[test]
fn reset_times_keep_offsets_and_fractional_seconds() {
    let at = |s: &str| {
        window(&json!({"utilization": 1, "resets_at": s}))
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        at("2026-11-16T12:00:00.396577+00:00")["resets_at"],
        1794830400u64
    );
    assert_eq!(at("2026-11-16T20:00:00+08:00")["resets_at"], 1794830400u64);
    assert!(window(&json!({"utilization": 1, "resets_at": "next week"})).is_err());
}
