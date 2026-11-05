//! A relay older than per-phone route codes would take the machine and route no phone to
//! it; `mooring remote --relay` leaves it and says the relay needs upgrading.
#[path = "support/tied.rs"]
mod tied;
use futures_util::{SinkExt, StreamExt};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tied::SpawnTied;
use tokio_tungstenite::tungstenite::Message;

#[test]
fn a_relay_that_only_speaks_protocol_2_is_left_with_a_reason() {
    let root = std::env::temp_dir().join(format!("mo-old-relay-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let addr = listener.local_addr().unwrap();
    // The old relay: takes any registration and answers that it speaks protocol 2.
    let relay = std::thread::spawn(move || {
        rt.block_on(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let register = ws.next().await.unwrap().unwrap();
            assert!(register.to_text().unwrap().contains("\"register\""));
            let registered = r#"{"t":"registered","proto":2}"#;
            ws.send(Message::text(registered)).await.unwrap();
            // The machine hangs up on its own.
            while let Some(Ok(m)) = ws.next().await {
                if m.is_close() {
                    break;
                }
            }
        })
    });
    let mut machine = Command::new(env!("CARGO_BIN_EXE_mooring"))
        .args(["remote", "--relay", &format!("ws://{addr}")])
        .args(["--runtime", &root.to_string_lossy()])
        .stderr(Stdio::piped())
        .spawn_tied()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = machine.try_wait().unwrap() {
            break status;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "still on a relay that speaks protocol 2 after 15 s"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut said = String::new();
    let mut stderr = machine.stderr.take().unwrap();
    stderr.read_to_string(&mut said).unwrap();
    relay.join().unwrap();
    assert!(!status.success(), "stayed on an old relay: {said}");
    assert!(said.contains("中继版本太旧（协议 2"), "{said}");
    let _ = std::fs::remove_dir_all(&root);
}
