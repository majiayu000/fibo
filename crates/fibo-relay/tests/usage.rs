//! Bytes per machine over real sockets: what a phone and its machine send each other shows
//! up on that machine's meter, and `/healthz` no longer says how many machines are on.
use fibo_relay::{
    server::{self, Config},
    usage::Usage,
};
use fibo_remote::wire::{self, bin, Frame};
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
const TOKEN: &str = "usageusageusageusageusageusage01";

async fn next(ws: &mut Ws) -> Message {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out")
        {
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            Some(Ok(m)) => return m,
            other => panic!("socket ended: {other:?}"),
        }
    }
}

async fn frame(ws: &mut Ws) -> Frame {
    match next(ws).await {
        Message::Text(t) => serde_json::from_str(&t).unwrap(),
        other => panic!("not a frame: {other:?}"),
    }
}

/// `(from_machine, to_machine)` from the one line `report` printed.
fn counts(lines: &[String]) -> (u64, u64) {
    assert_eq!(lines.len(), 1, "{lines:?}");
    let field = |name: &str| -> u64 {
        let tail = lines[0].split(&format!("{name}=")).nth(1).unwrap();
        tail.split(' ').next().unwrap().parse().unwrap()
    };
    (field("from_machine"), field("to_machine"))
}

#[tokio::test]
async fn what_a_phone_and_its_machine_send_is_counted_on_that_machine() {
    let usage = Arc::new(Usage::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::serve(
        listener,
        Config {
            usage: usage.clone(),
            ..Default::default()
        },
    ));
    let base = format!("ws://{addr}");

    let hash = wire::token_hash(TOKEN);
    let (mut m, _) = connect_async(format!("{base}/agent?th={hash}"))
        .await
        .unwrap();
    let register = Frame::Register {
        token_hash: hash.clone(),
        name: Some("test".into()),
        ver: None,
        key: None,
        proto: Some(2),
        retired: vec![],
        routes: vec![],
        revoked: vec![],
    };
    m.send(Message::text(register.to_text())).await.unwrap();
    assert_eq!(frame(&mut m).await, Frame::Registered { proto: Some(2) });

    let (mut p, _) = connect_async(format!("{base}/c/{TOKEN}/keepline"))
        .await
        .unwrap();
    let Frame::Wopen { id, .. } = frame(&mut m).await else {
        panic!()
    };
    m.send(Message::text(Frame::Wopened { id: id.clone() }.to_text()))
        .await
        .unwrap();

    p.send(Message::binary(vec![1u8; 10_000])).await.unwrap();
    assert_eq!(
        next(&mut m).await,
        Message::binary(wire::encode(bin::WBIN, &id, &[1u8; 10_000]))
    );
    m.send(Message::binary(wire::encode(
        bin::WBIN,
        &id,
        &[2u8; 30_000],
    )))
    .await
    .unwrap();
    assert_eq!(next(&mut p).await, Message::binary(vec![2u8; 30_000]));

    let lines = usage.report();
    assert!(lines[0].starts_with(&format!("usage machine={}", &hash[..12])));
    let (from, to) = counts(&lines);
    // The payloads plus a few hundred bytes of frames around them.
    assert!((30_000..31_000).contains(&from), "from_machine={from}");
    assert!((10_000..11_000).contains(&to), "to_machine={to}");
    assert!(usage.report().is_empty(), "counted once");

    let health: serde_json::Value = {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET /healthz HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        s.read_to_string(&mut body).await.unwrap();
        serde_json::from_str(body.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    };
    assert_eq!(health["ok"], true);
    assert!(health.get("agents").is_none(), "{health}");
}
