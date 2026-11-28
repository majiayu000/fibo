//! A relay older than per-phone route codes would take the machine and route no phone to
//! it; `fibo remote --relay` leaves it, says the relay needs upgrading, and stays to ask
//! again later (it may be upgraded without anyone touching this computer).
#[path = "support/tied.rs"]
mod tied;
use futures_util::{SinkExt, StreamExt};
use std::io::BufRead;
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
    let mut machine = Command::new(env!("CARGO_BIN_EXE_fibo"))
        .args(["remote", "--relay", &format!("ws://{addr}")])
        .args(["--runtime", &root.to_string_lossy()])
        .stderr(Stdio::piped())
        .spawn_tied()
        .unwrap();
    // What it says, line by line, until it names the old relay.
    let (said, heard) = std::sync::mpsc::channel();
    let stderr = machine.stderr.take().unwrap();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
        {
            let _ = said.send(line);
        }
    });
    let start = Instant::now();
    let reason = loop {
        let left = Duration::from_secs(15).saturating_sub(start.elapsed());
        match heard.recv_timeout(left) {
            Ok(line) if line.contains("中继版本太旧（协议 2") => break line,
            Ok(_) => continue,
            Err(_) => panic!("never said the relay is too old within 15 s"),
        }
    };
    assert!(reason.contains("分钟后再试"), "{reason}");
    // It hung up on the old relay, and waits to ask again rather than leaving for good.
    relay.join().unwrap();
    assert!(
        machine.try_wait().unwrap().is_none(),
        "left for good: {reason}"
    );
    let _ = machine.kill();
    let _ = machine.wait();
    let _ = std::fs::remove_dir_all(&root);
}
