use super::*;
use fibo_remote::secure::Channel;
use tokio_tungstenite::WebSocketStream;

type Relay = WebSocketStream<tokio::net::TcpStream>;

async fn fixture() -> (
    Relay,
    tokio::task::JoinHandle<(Ended, bool)>,
    std::path::PathBuf,
    String,
    Code,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join(format!("fibo-relay-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let socket = root.join("workbench.sock");
    let (phone, _) = devices::pair_link(
        &socket,
        "https://relay.test",
        "phone\x1b]2;title\x07\nforged",
        Role::Full,
    )
    .unwrap();
    let path = format!("{}?d={}", wire::MOORING_PATH, phone.id);
    let key = phone.key().unwrap();
    let connection = tokio::spawn(async move {
        connect(
            &format!("ws://{address}"),
            &Code::generate().unwrap(),
            None,
            "test",
            &socket,
            &Live::new(None),
        )
        .await
    });
    let (tcp, _) = listener.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
    assert!(matches!(from_machine(&mut ws).await, Message::Text(_)));
    ws.send(Message::text(
        Frame::Registered {
            proto: Some(wire::PROTOCOL),
        }
        .to_text(),
    ))
    .await
    .unwrap();
    (ws, connection, root, path, key)
}

async fn open(ws: &mut Relay, id: &str, path: &str) {
    ws.send(Message::text(
        Frame::Wopen {
            id: id.into(),
            path: path.into(),
        }
        .to_text(),
    ))
    .await
    .unwrap();
    assert_eq!(text_frame(ws).await, Frame::Wopened { id: id.into() });
}

async fn text_frame(ws: &mut Relay) -> Frame {
    let Message::Text(text) = from_machine(ws).await else {
        panic!("expected a text frame")
    };
    serde_json::from_str(&text).unwrap()
}

async fn handshake(ws: &mut Relay, id: &str, key: &Code) -> Channel {
    let (handshake, first) = secure::initiate(key).unwrap();
    ws.send(to_phone(wire::PROTOCOL, id, &first)).await.unwrap();
    let Message::Binary(answer) = from_machine(ws).await else {
        panic!("expected handshake answer")
    };
    let (_, to, answer) = wire::decode(&answer).unwrap();
    assert_eq!(to, id);
    handshake.finish(answer).unwrap()
}

/// The resource limit is process-wide: exercise it only in a fresh test process.
fn subprocess(test: &str) -> Option<std::process::Output> {
    let test = test.split_once("::").unwrap().1;
    if std::env::var("MOORING_RELAY_REGRESSION").as_deref() == Ok(test) {
        return None;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("MOORING_RELAY_REGRESSION", test)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "child did not execute its test"
    );
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Some(output)
}

#[test]
fn descriptor_exhaustion_closes_only_the_session_and_releases_partial_allocations() {
    let name = concat!(
        module_path!(),
        "::descriptor_exhaustion_closes_only_the_session_and_releases_partial_allocations"
    );
    if subprocess(name).is_some() {
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (mut ws, connection, root, path, key) = fixture().await;
            let mut original: libc::rlimit = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut original) },
                0
            );
            let low = libc::rlimit {
                rlim_cur: 128.min(original.rlim_cur),
                ..original
            };
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &low) }, 0);
            for spare in 0..5 {
                let id = format!("low-fd-{spare}");
                open(&mut ws, &id, &path).await;
                let (_, first) = secure::initiate(&key).unwrap();
                let mut held = Vec::new();
                while let Ok(file) = std::fs::File::open("/dev/null") {
                    held.push(file);
                }
                for _ in 0..spare {
                    held.pop().unwrap();
                }
                ws.send(to_phone(wire::PROTOCOL, &id, &first))
                    .await
                    .unwrap();
                assert_eq!(
                    text_frame(&mut ws).await,
                    Frame::Wclose {
                        id,
                        code: Some(close::NORMAL),
                        reason: Some("session unavailable".into()),
                    }
                );
                let mut recovered = Vec::new();
                while let Ok(file) = std::fs::File::open("/dev/null") {
                    recovered.push(file);
                }
                assert_eq!(recovered.len(), spare, "partial bridge leaked descriptors");
                assert!(!connection.is_finished());
                eprintln!("verified {spare} spare descriptors: Wclose, no leak, connection alive");
            }
            assert_eq!(
                unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &original) },
                0
            );
            open(&mut ws, "recovered", &path).await;
            handshake(&mut ws, "recovered", &key).await;
            eprintln!("verified a new handshake succeeds after descriptors recover");
            connection.abort();
            let _ = connection.await;
            std::fs::remove_dir_all(root).unwrap();
        });
}

#[tokio::test]
async fn replayed_handshake_expires_but_authenticated_idle_session_survives() {
    let (mut ws, connection, root, path, key) = fixture().await;
    open(&mut ws, "pairing", &path).await;
    let (_, replay) = secure::initiate(&key).unwrap();
    for id in ["replayed-1", "replayed-2"] {
        open(&mut ws, id, &path).await;
        // The relay knows these exact captured bytes, but not either channel's key.
        ws.send(to_phone(wire::PROTOCOL, id, &replay))
            .await
            .unwrap();
        assert!(matches!(from_machine(&mut ws).await, Message::Binary(_)));
    }
    open(&mut ws, "authenticated", &path).await;
    let mut channel = handshake(&mut ws, "authenticated", &key).await;
    ws.send(to_phone(
        wire::PROTOCOL,
        "authenticated",
        &channel.ack(0).unwrap(),
    ))
    .await
    .unwrap();
    // A reply to the next frame proves the authenticated message was consumed first.
    ws.send(Message::text(
        Frame::Hreq {
            id: "barrier".into(),
        }
        .to_text(),
    ))
    .await
    .unwrap();
    assert!(matches!(text_frame(&mut ws).await, Frame::Hres { .. }));
    assert!(matches!(text_frame(&mut ws).await, Frame::Hend { .. }));
    tokio::time::pause();
    for _ in 0..HANDSHAKE_LIMIT.as_secs() + 2 {
        tokio::time::advance(RECHECK).await;
        tokio::task::yield_now().await;
    }
    tokio::time::resume();
    let mut closed = Vec::new();
    for _ in 0..3 {
        let Frame::Wclose { id, code, reason } = text_frame(&mut ws).await else {
            panic!("expected the unauthenticated session to expire")
        };
        assert_eq!(code, Some(close::NORMAL));
        assert_eq!(reason.as_deref(), Some("handshake timeout"));
        closed.push(id);
    }
    closed.sort();
    assert_eq!(closed, ["pairing", "replayed-1", "replayed-2"]);
    ws.send(to_phone(wire::PROTOCOL, "authenticated", b"invalid"))
        .await
        .unwrap();
    assert!(matches!(text_frame(&mut ws).await,
        Frame::Wclose { id, code: Some(close::BAD_MESSAGE), .. } if id == "authenticated"));
    assert!(!connection.is_finished());
    eprintln!("verified pairing/replay expired; authenticated idle session remained open");
    connection.abort();
    let _ = connection.await;
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn relay_control_characters_are_escaped_in_actual_stderr() {
    let name = concat!(
        module_path!(),
        "::relay_control_characters_are_escaped_in_actual_stderr"
    );
    if let Some(output) = subprocess(name) {
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            !stderr.contains('\x1b'),
            "raw ESC reached stderr: {stderr:?}"
        );
        assert!(
            !stderr.contains('\x07'),
            "raw BEL reached stderr: {stderr:?}"
        );
        assert!(!stderr.contains('\r'), "raw CR reached stderr: {stderr:?}");
        assert!(
            !stderr.contains("\nforged"),
            "newline injected a log line: {stderr:?}"
        );
        for value in [
            "phone\\u{1b}]2;title\\u{7}\\nforged",
            "id\\u{1b}[2J\\r\\nforged",
            "device\\u{1b}[2J\\nforged",
        ] {
            assert!(
                stderr.contains(value),
                "missing escaped field {value:?}: {stderr}"
            );
        }
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (mut ws, connection, root, path, key) = fixture().await;
            let id = "id\x1b[2J\r\nforged";
            open(&mut ws, id, &path).await;
            handshake(&mut ws, id, &key).await;
            ws.send(to_phone(wire::PROTOCOL, id, b"invalid"))
                .await
                .unwrap();
            assert!(matches!(text_frame(&mut ws).await, Frame::Wclose { .. }));
            let path = format!("{}?d=device\x1b[2J\nforged", wire::MOORING_PATH);
            open(&mut ws, "unknown", &path).await;
            let (_, first) = secure::initiate(&key).unwrap();
            ws.send(to_phone(wire::PROTOCOL, "unknown", &first))
                .await
                .unwrap();
            assert!(matches!(text_frame(&mut ws).await, Frame::Wclose { .. }));
            connection.abort();
            let _ = connection.await;
            std::fs::remove_dir_all(root).unwrap();
        });
}

#[tokio::test]
async fn relay_refusal_is_safe_when_returned_to_the_cli() {
    use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join(format!("fibo-relay-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let socket = root.join("workbench.sock");
    let connection = tokio::spawn(async move {
        connect(
            &format!("ws://{address}"),
            &Code::generate().unwrap(),
            None,
            "test",
            &socket,
            &Live::new(None),
        )
        .await
    });
    let (tcp, _) = listener.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
    assert!(matches!(from_machine(&mut ws).await, Message::Text(_)));
    ws.send(Message::text(
        Frame::Error {
            message: "refused\x1b[2J\r\nforged".into(),
        }
        .to_text(),
    ))
    .await
    .unwrap();
    ws.send(Message::Close(Some(CloseFrame {
        code: CloseCode::from(4002),
        reason: "".into(),
    })))
    .await
    .unwrap();
    let (ended, was_up) = connection.await.unwrap();
    assert!(!was_up);
    let Ended::Refused(why) = ended else {
        panic!("expected a refusal")
    };
    assert!(
        !why.chars().any(char::is_control),
        "raw controls returned to CLI: {why:?}"
    );
    assert!(why.contains("refused\\u{1b}[2J\\r\\nforged"));
    std::fs::remove_dir_all(root).unwrap();
}
