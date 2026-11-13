//! #69: the machine's token hash is all it takes to register as it, so it stays out of the
//! URL proxies log, and a machine pushed off its code by a connection elsewhere takes the
//! code back later instead of leaving for good.
use super::*;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};

/// Dials a fake relay, registers, and has the relay close with 4000. Before the close,
/// `before` may rewrite the owner file as another process would. Returns the URL the
/// machine asked for and how the connection ended.
// The callback signature is tungstenite's; its error type is not ours to shrink.
#[allow(clippy::result_large_err)]
async fn replaced(before: impl FnOnce(&Path)) -> (String, Ended, bool) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join(format!("fibo-relay-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let socket = root.join("workbench.sock");
    let dialing = socket.clone();
    let connection = tokio::spawn(async move {
        connect(
            &format!("ws://{address}"),
            &Code::generate().unwrap(),
            None,
            "test",
            &dialing,
            &Live::new(None),
        )
        .await
    });
    let (tcp, _) = listener.accept().await.unwrap();
    let asked = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = asked.clone();
    let mut ws = tokio_tungstenite::accept_hdr_async(tcp, move |req: &Request, res: Response| {
        *seen.lock().unwrap() = req.uri().to_string();
        Ok(res)
    })
    .await
    .unwrap();
    let Message::Text(register) = from_machine(&mut ws).await else {
        panic!("expected Register")
    };
    assert!(matches!(
        serde_json::from_str::<Frame>(&register),
        Ok(Frame::Register { .. })
    ));
    ws.send(Message::text(
        Frame::Registered {
            proto: Some(wire::PROTOCOL),
        }
        .to_text(),
    ))
    .await
    .unwrap();
    before(&socket);
    ws.send(Message::Close(Some(CloseFrame {
        code: CloseCode::from(4000),
        reason: "Replaced by a newer connection".into(),
    })))
    .await
    .unwrap();
    let (ended, was_up) = connection.await.unwrap();
    std::fs::remove_dir_all(root).unwrap();
    let url = asked.lock().unwrap().clone();
    (url, ended, was_up)
}

#[tokio::test]
async fn the_token_hash_is_not_in_the_url() {
    let (url, _, _) = replaced(|_| ()).await;
    assert_eq!(url, wire::AGENT_PATH, "URL 里不该有查询串");
}

#[tokio::test]
async fn pushed_off_from_elsewhere_it_takes_the_code_back_later() {
    let (_, ended, was_up) = replaced(|_| ()).await;
    assert!(was_up);
    assert!(matches!(ended, Ended::Replaced));
    let mut wait = Duration::from_secs(16);
    let (pause, why) = after(ended, was_up, &mut wait).expect("被顶掉后不该退出");
    assert_eq!(pause, REPLACED_WAIT);
    assert!(why.contains("顶了下来"), "{why}");
    assert_eq!(wait, Duration::from_secs(1));
}

/// A newer process on this machine (a host restarted beside its old child) still has the
/// old one leave, as before: otherwise the two would push each other off forever.
#[tokio::test]
async fn pushed_off_by_a_newer_process_here_it_leaves() {
    let mut other = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = other.id();
    let (_, ended, _) = replaced(|socket| {
        std::fs::write(socket.with_file_name("remote-relay.owner"), pid.to_string()).unwrap();
    })
    .await;
    let _ = other.kill();
    let _ = other.wait();
    let Ended::Refused(why) = ended else {
        panic!("本机更新的进程接管时旧的应该退出")
    };
    assert!(why.contains("这台电脑上"), "{why}");
}

/// An owner that has since died is no reason to leave.
#[tokio::test]
async fn an_owner_no_longer_running_is_not_a_takeover() {
    let mut gone = std::process::Command::new("true").spawn().unwrap();
    let pid = gone.id();
    gone.wait().unwrap();
    let (_, ended, _) = replaced(|socket| {
        std::fs::write(socket.with_file_name("remote-relay.owner"), pid.to_string()).unwrap();
    })
    .await;
    assert!(matches!(ended, Ended::Replaced));
}
