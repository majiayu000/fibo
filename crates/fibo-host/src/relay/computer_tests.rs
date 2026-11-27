//! Another computer through the relay: its session reaches this machine's own socket, and
//! only the pairing record decides that.
use super::*;
use crate::relay::squeeze::{Squeeze, Unsqueeze};
use fibo_remote::secure::Message as Secure;
use std::io::{Read, Write};

const FLOOD: usize = 8 << 20;

/// Runs the real connection loop against a fake relay; a stand-in for the native host on
/// the socket echoes what it reads.
#[tokio::test]
async fn a_computer_reaches_the_native_socket_and_only_with_full_control() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join(format!("fr-{}", &uuid::Uuid::new_v4().to_string()[..8]));
    std::fs::create_dir_all(&root).unwrap();
    let socket = root.join("workbench.sock");
    let native = fibo_platform::local::Listener::bind(&socket).unwrap();
    std::thread::spawn(move || {
        while let Ok(mut s) = native.accept() {
            // Room for whole `secure::CHUNK`s to be read at once there, as on Linux.
            let _ = s.set_send_buffer(1 << 20);
            std::thread::spawn(move || {
                let mut buf = [0; 64];
                let mut seed = 0x9e37_79b9_u32;
                while let Ok(n @ 1..) = s.read(&mut buf) {
                    // What a workspace or a busy terminal does: much at once, in small writes.
                    // Noise, so it does not compress away.
                    if &buf[..n] == b"flood" {
                        // Big writes, so reads there fill whole `secure::CHUNK`s.
                        for _ in 0..FLOOD / 65536 {
                            let block: Vec<u8> = (0..65536)
                                .map(|_| {
                                    seed ^= seed << 13;
                                    seed ^= seed >> 17;
                                    seed ^= seed << 5;
                                    seed as u8
                                })
                                .collect();
                            if s.write_all(&block).is_err() {
                                return;
                            }
                        }
                        continue;
                    }
                    let mut echo = b"native:".to_vec();
                    echo.extend(&buf[..n]);
                    if s.write_all(&echo).is_err() {
                        break;
                    }
                }
            });
        }
    });
    let relay = "https://relay.test";
    let (computer, _) = devices::pair_computer_link(&socket, relay, &[], "mac1").unwrap();
    let (narrowed, _) = devices::pair_computer_link(&socket, relay, &[], "other").unwrap();
    devices::set_role(&socket, &narrowed.id, Role::View).unwrap();
    let (phone, _) = devices::pair_link(&socket, relay, &[], "phone", Role::Full).unwrap();
    // On a runtime of its own: stuck, it must not stop this test's timeout from firing.
    let connection = {
        let socket = socket.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(connect(
                &format!("ws://{address}"),
                &Code::generate().unwrap(),
                None,
                "test",
                &socket,
                &Live::new(None),
            ))
        })
    };
    let (tcp, _) = listener.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
    assert!(matches!(from_machine(&mut ws).await, Message::Text(_)));
    let registered = Frame::Registered {
        proto: Some(wire::PROTOCOL),
    };
    ws.send(Message::text(registered.to_text())).await.unwrap();

    // Opens a session as `device` and finishes its handshake.
    async fn open<S>(ws: &mut S, id: &str, device: &Device) -> Option<secure::Channel>
    where
        S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
            + futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
            + Unpin,
    {
        let path = format!("{}?d={}", wire::MOORING_PATH, device.id);
        let wopen = Frame::Wopen {
            id: id.into(),
            path,
        };
        ws.send(Message::text(wopen.to_text())).await.unwrap();
        assert!(matches!(from_machine(ws).await, Message::Text(_)));
        let (handshake, first) = secure::initiate(&device.key().unwrap()).unwrap();
        let first = Message::binary(wire::encode(bin::WBIN, id, &first));
        ws.send(first).await.unwrap();
        match from_machine(ws).await {
            Message::Binary(b) => {
                let (_, _, answer) = wire::decode(&b).unwrap();
                Some(handshake.finish(answer).unwrap())
            }
            Message::Text(t) => match serde_json::from_str::<Frame>(&t).unwrap() {
                Frame::Wclose { code, .. } => {
                    assert_eq!(code, Some(close::UNPAIRED));
                    None
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }
    async fn data<S>(ws: &mut S, channel: &mut secure::Channel) -> Vec<u8>
    where
        S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
            + Unpin,
    {
        loop {
            let Message::Binary(b) = from_machine(ws).await else {
                continue;
            };
            let (_, _, sealed) = wire::decode(&b).unwrap();
            if let Secure::Data(bytes) = channel.open(sealed).unwrap() {
                return bytes;
            }
        }
    }

    let devices = devices::load(&socket);
    let find = |id: &str| devices.iter().find(|d| d.id == id).unwrap().clone();

    // The computer is told where it arrived, then speaks to the native socket.
    let mut channel = open(&mut ws, "c1", &find(&computer.id)).await.unwrap();
    let about: crate::relay::computer::About =
        serde_json::from_slice(&data(&mut ws, &mut channel).await).unwrap();
    assert_eq!(about.machine, devices::machine(&socket).unwrap().id);
    // After it, compressed both ways.
    let (mut squeeze, mut unsqueeze) = (Squeeze::new().unwrap(), Unsqueeze::new().unwrap());
    let hello = channel.data(&squeeze.pack(b"hello").unwrap()).unwrap();
    ws.send(Message::binary(wire::encode(bin::WBIN, "c1", &hello)))
        .await
        .unwrap();
    let echoed = data(&mut ws, &mut channel).await;
    assert_eq!(unsqueeze.unpack(&echoed).unwrap(), b"native:hello");

    // Many times the window at once, acknowledged as it arrives, as `dial` does: the loop
    // that opens ACKs must never wait on a session stuck behind the queue it drains (it
    // did, and the machine fell off the relay with the window's first workspace).
    let flood = channel.data(&squeeze.pack(b"flood").unwrap()).unwrap();
    ws.send(Message::binary(wire::encode(bin::WBIN, "c1", &flood)))
        .await
        .unwrap();
    let mut got = 0;
    let all = tokio::time::timeout(Duration::from_secs(20), async {
        while got < FLOOD {
            let packed = data(&mut ws, &mut channel).await;
            got += unsqueeze.unpack(&packed).unwrap().len();
            let ack = channel.ack(packed.len() as u32).unwrap();
            ws.send(Message::binary(wire::encode(bin::WBIN, "c1", &ack)))
                .await
                .unwrap();
        }
    })
    .await;
    assert!(all.is_ok(), "stalled after {got} of {FLOOD} bytes");

    // One turned down from full control is refused, not given the phone protocol.
    assert!(open(&mut ws, "c2", &find(&narrowed.id)).await.is_none());

    // A phone, full control too, gets the phone protocol: no About, and its request is
    // answered as one, never echoed by the native socket.
    let mut channel = open(&mut ws, "p1", &find(&phone.id)).await.unwrap();
    let request = channel
        .data(b"{\"id\":1,\"op\":\"hello\",\"version\":2}\n")
        .unwrap();
    ws.send(Message::binary(wire::encode(bin::WBIN, "p1", &request)))
        .await
        .unwrap();
    let reply = data(&mut ws, &mut channel).await;
    assert!(
        !reply.starts_with(b"native:"),
        "{}",
        String::from_utf8_lossy(&reply)
    );
    assert!(serde_json::from_slice::<crate::relay::computer::About>(&reply).is_err());

    let log = std::fs::read_to_string(devices::log_file(&socket)).unwrap();
    assert!(log.contains("\"kind\":\"computer\""), "{log}");
    assert!(log.contains("computer without full control"), "{log}");
    assert!(!connection.is_finished(), "the connection loop ended");
    std::fs::remove_dir_all(root).unwrap();
}
