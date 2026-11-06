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

/// The next frame from the machine; a machine that died ends the socket instead.
async fn from_machine<S>(ws: &mut S) -> Message
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(Message::Ping(_)))) => (),
            Ok(Some(Ok(m))) => return m,
            other => panic!("the machine stopped answering: {other:?}"),
        }
    }
}

/// A relay is not trusted, and it picks the ids. Runs the real connection loop against a
/// relay that replays a paired phone's handshake under each id, as the issue describes.
#[tokio::test]
async fn a_frame_id_that_does_not_fit_is_refused_and_one_that_just_fits_works() {
    use base64::Engine;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join(format!("mooring-relay-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let socket = root.join("workbench.sock");
    let (phone, _) =
        devices::pair_link(&socket, "https://relay.test", "phone", Role::Full).unwrap();
    let key = phone.key().unwrap();
    let path = format!("{}?d={}", wire::MOORING_PATH, phone.id);
    let connection = {
        let socket = socket.clone();
        tokio::spawn(async move {
            connect(
                &format!("ws://{address}"),
                &Code::generate().unwrap(),
                None,
                "test",
                &socket,
                &Live::new(None),
            )
            .await
        })
    };
    let (tcp, _) = listener.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
    assert!(matches!(from_machine(&mut ws).await, Message::Text(_)));
    let registered = Frame::Registered {
        proto: Some(wire::PROTOCOL),
    };
    ws.send(Message::text(registered.to_text())).await.unwrap();
    let wopen = |id: &str| {
        let frame = Frame::Wopen {
            id: id.into(),
            path: path.clone(),
        };
        Message::text(frame.to_text())
    };
    let json_first = |id: &str, first: &[u8]| {
        let frame = Frame::Wmsg {
            id: id.into(),
            s: None,
            b: Some(base64::engine::general_purpose::STANDARD.encode(first)),
        };
        Message::text(frame.to_text())
    };

    // Bytes count, not characters: the last two are well under 255 characters.
    for id in [
        "a".repeat(256),
        format!("{}a", "中".repeat(85)),
        "中".repeat(86),
    ] {
        assert!(id.len() > wire::MAX_ID_LEN);
        ws.send(wopen(&id)).await.unwrap();
        let reply = from_machine(&mut ws).await;
        let Message::Text(reply) = reply else {
            panic!("{reply:?}")
        };
        assert_eq!(
            serde_json::from_str::<Frame>(&reply).unwrap(),
            Frame::Werr {
                id: id.clone(),
                message: "id too long".into()
            }
        );
        // No session: this handshake gets no answer. Frames are handled in order, so the
        // reply to the next Wopen below would come after any answer to this one.
        let (_, first) = secure::initiate(&key).unwrap();
        ws.send(json_first(&id, &first)).await.unwrap();
    }

    // Exactly 255 bytes finish the handshake, with the first message in either form.
    for (id, binary) in [("a".repeat(255), false), ("中".repeat(85), true)] {
        assert_eq!(id.len(), wire::MAX_ID_LEN);
        ws.send(wopen(&id)).await.unwrap();
        let opened = from_machine(&mut ws).await;
        let Message::Text(opened) = opened else {
            panic!("{opened:?}")
        };
        assert_eq!(
            serde_json::from_str::<Frame>(&opened).unwrap(),
            Frame::Wopened { id: id.clone() }
        );
        let (handshake, first) = secure::initiate(&key).unwrap();
        let first = if binary {
            Message::binary(wire::encode(bin::WBIN, &id, &first))
        } else {
            json_first(&id, &first)
        };
        ws.send(first).await.unwrap();
        let answer = from_machine(&mut ws).await;
        let Message::Binary(answer) = answer else {
            panic!("{answer:?}")
        };
        let (kind, to, answer) = wire::decode(&answer).unwrap();
        assert_eq!((kind, to), (bin::WBIN, id.as_str()));
        handshake.finish(answer).unwrap();
    }

    assert!(!connection.is_finished(), "the connection loop ended");
    connection.abort();
    let _ = connection.await;
    std::fs::remove_dir_all(root).unwrap();
}

#[path = "regressions.rs"]
mod regressions;
