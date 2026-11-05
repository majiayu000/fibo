use super::*;

fn device(id: &str, role: Role, kicked: u64) -> Device {
    serde_json::from_value(json!({
        "id": id, "name": id, "key": "", "role": role, "paired": 0, "kicked": kicked,
    }))
    .unwrap()
}

#[test]
fn each_reason_a_session_ends_has_its_own_close_code() {
    let since = 1_000;
    let now = [device("a", Role::Full, 0), device("b", Role::View, 0)];
    assert_eq!(dropped(&now, "a", Role::Full, since), None);

    // Unpaired: the phone must pair again.
    assert_eq!(
        dropped(&now, "gone", Role::Full, since),
        Some(Dropped::Unpaired)
    );
    assert_eq!(Dropped::Unpaired.close().0, close::UNPAIRED);

    // "Disconnect" after the session opened ends it; one from before does not.
    let kicked = [device("a", Role::Full, since + 1)];
    assert_eq!(
        dropped(&kicked, "a", Role::Full, since),
        Some(Dropped::Kicked)
    );
    assert_eq!(Dropped::Kicked.close().0, close::KICKED);
    let earlier = [device("a", Role::Full, since - 1)];
    assert_eq!(dropped(&earlier, "a", Role::Full, since), None);

    // A new role: reconnect to get it.
    assert_eq!(
        dropped(&now, "b", Role::Full, since),
        Some(Dropped::RoleChanged)
    );
    assert_eq!(Dropped::RoleChanged.close().0, close::ROLE_CHANGED);

    // Three different codes; only unpairing is the "refused" one.
    let codes = [Dropped::Unpaired, Dropped::Kicked, Dropped::RoleChanged].map(|d| d.close().0);
    assert_eq!(codes, [4401, 4410, 4411]);
}

/// Run the real connection loop against a relay which registers and then sends nothing.
/// Advancing the clock keeps the device recheck tick active without a 75-second test.
#[tokio::test]
async fn device_rechecks_do_not_extend_the_silence_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
        let registration = ws.next().await.unwrap().unwrap();
        assert!(matches!(registration, Message::Text(_)));
        ws.send(Message::text(
            Frame::Registered {
                proto: Some(wire::PROTOCOL),
            }
            .to_text(),
        ))
        .await
        .unwrap();
        // Keep the TCP connection alive without reading or answering Ping frames.
        std::future::pending::<()>().await;
        drop(ws);
    });
    let root = std::env::temp_dir().join(format!("mooring-relay-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let socket = root.join("workbench.sock");
    let live = Live::new(None);
    let state = live.clone();
    let connection = tokio::spawn(async move {
        connect(
            &format!("ws://{address}"),
            &Code::generate().unwrap(),
            None,
            "test",
            &socket,
            &live,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while !state.connected() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::pause();
    for _ in 0..SILENCE.as_secs() + 2 {
        tokio::time::advance(RECHECK).await;
        tokio::task::yield_now().await;
    }
    assert!(
        connection.is_finished(),
        "device ticks kept the silent connection alive"
    );
    let (ended, was_up) = connection.await.unwrap();
    assert!(was_up);
    assert!(matches!(ended, Ended::Lost(ref why) if why == "中继很久没有消息"));
    server.abort();
    let _ = server.await;
    std::fs::remove_dir_all(root).unwrap();
}
