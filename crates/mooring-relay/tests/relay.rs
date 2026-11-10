//! The relay end to end over real sockets: a fake machine and fake phones.
use futures_util::{SinkExt, StreamExt};
use mooring_relay::server::{self, Config};
use mooring_remote::{
    close,
    wire::{self, bin, Frame},
};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
const TOKEN: &str = "abcdefghijklmnopqrstuvwxyz012345";

async fn relay(config: Config) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::serve(listener, config));
    format!("ws://{addr}")
}

async fn machine(base: &str, token: &str, key: Option<&str>) -> Ws {
    machine_retiring(base, token, key, &[]).await
}

async fn machine_retiring(base: &str, token: &str, key: Option<&str>, retired: &[&str]) -> Ws {
    let hash = wire::token_hash(token);
    let (mut ws, _) = connect_async(format!("{base}/agent?th={hash}"))
        .await
        .unwrap();
    let register = Frame::Register {
        token_hash: hash,
        name: Some("test".into()),
        ver: None,
        key: key.map(Into::into),
        proto: Some(2),
        retired: retired.iter().map(|t| wire::token_hash(t)).collect(),
        routes: vec![],
        revoked: vec![],
    };
    ws.send(Message::text(register.to_text())).await.unwrap();
    ws
}

async fn registered(base: &str, token: &str) -> Ws {
    let mut ws = machine(base, token, None).await;
    assert_eq!(frame(&mut ws).await, Frame::Registered { proto: Some(2) });
    ws
}

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

async fn closed(ws: &mut Ws) -> u16 {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("not closed")
        {
            Some(Ok(Message::Close(f))) => return f.map_or(1005, |f| u16::from(f.code)),
            Some(Ok(_)) => continue,
            _ => return 1006,
        }
    }
}

async fn phone(base: &str, token: &str) -> Ws {
    connect_async(format!("{base}/c/{token}/keepline"))
        .await
        .unwrap()
        .0
}

#[tokio::test]
async fn a_phone_reaches_its_machine_both_ways_and_closes_reach_the_other_side() {
    let base = relay(Config::default()).await;
    let mut m = registered(&base, TOKEN).await;

    let mut p = phone(&base, TOKEN).await;
    let Frame::Wopen { id, path } = frame(&mut m).await else {
        panic!()
    };
    assert_eq!(path, "/keepline");
    // Sent before the machine answered: held, then delivered in order.
    p.send(Message::binary(b"first".to_vec())).await.unwrap();
    p.send(Message::text("second")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    m.send(Message::text(Frame::Wopened { id: id.clone() }.to_text()))
        .await
        .unwrap();
    assert_eq!(
        next(&mut m).await,
        Message::binary(wire::encode(bin::WBIN, &id, b"first"))
    );
    assert_eq!(
        next(&mut m).await,
        Message::binary(wire::encode(bin::WTEXT, &id, b"second"))
    );

    m.send(Message::binary(wire::encode(bin::WBIN, &id, b"\x00back")))
        .await
        .unwrap();
    assert_eq!(next(&mut p).await, Message::binary(b"\x00back".to_vec()));
    m.send(Message::text(
        Frame::Wmsg {
            id: id.clone(),
            s: Some("json form".into()),
            b: None,
        }
        .to_text(),
    ))
    .await
    .unwrap();
    assert_eq!(next(&mut p).await, Message::text("json form"));

    // The phone leaves: the machine hears it.
    p.close(None).await.unwrap();
    let Frame::Wclose { id: gone, .. } = frame(&mut m).await else {
        panic!()
    };
    assert_eq!(gone, id);

    // The machine closes a second phone, with its own code.
    let mut p = phone(&base, TOKEN).await;
    let Frame::Wopen { id, .. } = frame(&mut m).await else {
        panic!()
    };
    m.send(Message::text(Frame::Wopened { id: id.clone() }.to_text()))
        .await
        .unwrap();
    m.send(Message::text(
        Frame::Wclose {
            id,
            code: Some(4100),
            reason: Some("bye".into()),
        }
        .to_text(),
    ))
    .await
    .unwrap();
    assert_eq!(closed(&mut p).await, 4100);

    // A plain-HTTP request the machine may get from an upstream relay is just another frame.
    let health: serde_json::Value = {
        let addr = base.trim_start_matches("ws://");
        let mut s = TcpStream::connect(addr).await.unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        s.write_all(b"GET /healthz HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        s.read_to_string(&mut body).await.unwrap();
        serde_json::from_str(body.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    };
    assert_eq!(health["ok"], true);
    assert_eq!(health["protocol"], wire::PROTOCOL);
}

#[tokio::test]
async fn unknown_tokens_bad_keys_and_bad_hashes_are_refused() {
    let base = relay(Config {
        key: Some("k".into()),
        ..Default::default()
    })
    .await;
    let status = |e: tokio_tungstenite::tungstenite::Error| match e {
        tokio_tungstenite::tungstenite::Error::Http(r) => r.status().as_u16(),
        other => panic!("{other:?}"),
    };
    let e = connect_async(format!("{base}/c/{TOKEN}/keepline"))
        .await
        .unwrap_err();
    assert_eq!(status(e), 502);
    let e = connect_async(format!("{base}/c/short/mooring"))
        .await
        .unwrap_err();
    assert_eq!(status(e), 404);

    let mut m = machine(&base, TOKEN, Some("wrong")).await;
    assert!(matches!(frame(&mut m).await, Frame::Error { .. }));
    assert_eq!(closed(&mut m).await, 4003);

    let (mut m, _) = connect_async(format!("{base}/agent")).await.unwrap();
    let bad = Frame::Register {
        token_hash: "nothex".into(),
        name: None,
        ver: None,
        key: Some("k".into()),
        proto: None,
        retired: vec![],
        routes: vec![],
        revoked: vec![],
    };
    m.send(Message::text(bad.to_text())).await.unwrap();
    assert!(matches!(frame(&mut m).await, Frame::Error { .. }));
    assert_eq!(closed(&mut m).await, 4002);

    let mut m = machine(&base, TOKEN, Some("k")).await;
    // No proto offered would be 1; this one offers 2.
    assert_eq!(frame(&mut m).await, Frame::Registered { proto: Some(2) });
}

#[tokio::test]
async fn a_phone_on_a_retired_token_is_told_to_pair_again_while_its_machine_is_on() {
    const OLD: &str = "oldoldoldoldoldoldoldoldoldold01";
    let base = relay(Config::default()).await;
    let mut m = machine_retiring(&base, TOKEN, None, &[OLD]).await;
    assert_eq!(frame(&mut m).await, Frame::Registered { proto: Some(2) });

    // The old token gets a socket that closes with its own code, which a phone can read
    // (an HTTP status before the upgrade it cannot).
    let mut p = phone(&base, OLD).await;
    assert_eq!(closed(&mut p).await, close::UNKNOWN_MACHINE);
    // The machine's current token still reaches it.
    let _p = phone(&base, TOKEN).await;
    assert!(matches!(frame(&mut m).await, Frame::Wopen { .. }));

    // Once the machine is gone the old token is only unknown again: it may just be offline.
    drop(m);
    drop(_p);
    let status = async || loop {
        match connect_async(format!("{base}/c/{OLD}/keepline")).await {
            Err(tokio_tungstenite::tungstenite::Error::Http(r)) => break r.status().as_u16(),
            Ok((mut p, _)) => {
                closed(&mut p).await;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => panic!("{e:?}"),
        }
    };
    let got = tokio::time::timeout(Duration::from_secs(5), status())
        .await
        .expect("the retired token outlived its machine");
    assert_eq!(got, 502);
}

#[tokio::test]
async fn a_newer_machine_replaces_the_old_and_a_gone_machine_closes_its_phones() {
    let base = relay(Config::default()).await;
    let mut old = registered(&base, TOKEN).await;
    let mut p = phone(&base, TOKEN).await;
    let Frame::Wopen { .. } = frame(&mut old).await else {
        panic!()
    };

    let mut new = registered(&base, TOKEN).await;
    assert_eq!(closed(&mut old).await, 4000);
    assert_eq!(closed(&mut p).await, 1012);

    let _p = phone(&base, TOKEN).await;
    let Frame::Wopen { .. } = frame(&mut new).await else {
        panic!()
    };
    drop(new);
    let mut p = _p;
    assert_eq!(closed(&mut p).await, 1012);
}

#[tokio::test]
async fn a_phone_that_stops_reading_is_cut_and_never_slows_its_neighbour() {
    let base = relay(Config {
        phone_buffer: 4 << 20,
        ..Default::default()
    })
    .await;
    let mut m = registered(&base, TOKEN).await;
    let open = async |m: &mut Ws| {
        let p = phone(&base, TOKEN).await;
        let Frame::Wopen { id, .. } = frame(m).await else {
            panic!()
        };
        m.send(Message::text(Frame::Wopened { id: id.clone() }.to_text()))
            .await
            .unwrap();
        (p, id)
    };
    let (_stalled, stalled_id) = open(&mut m).await;
    let (mut live, live_id) = open(&mut m).await;

    let big = vec![7u8; 1 << 20];
    let mut worst = Duration::ZERO;
    let mut cut = false;
    for i in 0..64 {
        m.send(Message::binary(wire::encode(bin::WBIN, &stalled_id, &big)))
            .await
            .unwrap();
        let at = Instant::now();
        m.send(Message::binary(wire::encode(bin::WBIN, &live_id, &[i])))
            .await
            .unwrap();
        assert_eq!(next(&mut live).await, Message::binary(vec![i]));
        worst = worst.max(at.elapsed());
        // The relay tells the machine once it gives up on the stalled phone.
        while let Ok(Some(Ok(Message::Text(t)))) =
            tokio::time::timeout(Duration::from_millis(1), m.next()).await
        {
            if let Ok(Frame::Wclose { id, code, .. }) = serde_json::from_str(&t) {
                assert_eq!((id.as_str(), code), (stalled_id.as_str(), Some(1013)));
                cut = true;
            }
        }
        if cut {
            break;
        }
    }
    assert!(cut, "the stalled phone was never cut");
    assert!(
        worst < Duration::from_millis(200),
        "neighbour waited {worst:?}"
    );
    eprintln!("neighbour's worst round trip beside a stalled phone: {worst:?}");
}

/// A phone that vanished without a close (battery out, NAT dropped) is noticed by the
/// pings it no longer answers, and its machine is told, so it stops holding one of its
/// few phone slots and showing it online.
#[tokio::test]
async fn a_phone_that_stops_answering_pings_is_closed_and_its_machine_told() {
    let base = relay(Config {
        ping: Duration::from_millis(200),
        ..Default::default()
    })
    .await;
    let mut m = registered(&base, TOKEN).await;
    let _silent = phone(&base, TOKEN).await; // never read again: never pongs
    let Frame::Wopen { id, .. } = frame(&mut m).await else {
        panic!()
    };
    m.send(Message::text(Frame::Wopened { id: id.clone() }.to_text()))
        .await
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        assert!(Instant::now() < until, "the silent phone was never closed");
        if let Ok(Some(Ok(Message::Text(t)))) =
            tokio::time::timeout(Duration::from_millis(100), m.next()).await
        {
            if let Ok(Frame::Wclose { id: gone, .. }) = serde_json::from_str(&t) {
                assert_eq!(gone, id);
                break;
            }
        }
    }
}

/// Stage 4: on protocol 3 each phone reaches its machine only by its own route. The
/// token the machine registers with routes no phone, so no phone can take its place;
/// an unpaired phone is told so and the others carry on.
#[tokio::test]
async fn phones_reach_a_machine_only_by_their_own_routes() {
    const A: &str = "phone-a-route-token-0123456789ab";
    const B: &str = "phone-b-route-token-0123456789ab";
    let base = relay(Config::default()).await;
    let hash = wire::token_hash(TOKEN);
    let (mut m, _) = connect_async(format!("{base}/agent?th={hash}"))
        .await
        .unwrap();
    let register = Frame::Register {
        token_hash: hash,
        name: None,
        ver: None,
        key: None,
        proto: Some(3),
        retired: vec![],
        routes: vec![wire::token_hash(A), wire::token_hash(B)],
        revoked: vec![],
    };
    m.send(Message::text(register.to_text())).await.unwrap();
    assert_eq!(frame(&mut m).await, Frame::Registered { proto: Some(3) });

    // The machine's own token is no way in for a phone.
    assert!(connect_async(format!("{base}/c/{TOKEN}/keepline"))
        .await
        .is_err());
    let _a = phone(&base, A).await;
    assert!(matches!(frame(&mut m).await, Frame::Wopen { .. }));

    // Phone A unpaired: the machine sends its new list.
    let routes = Frame::Routes {
        routes: vec![wire::token_hash(B)],
        revoked: vec![wire::token_hash(A)],
    };
    m.send(Message::text(routes.to_text())).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut again = phone(&base, A).await;
    assert_eq!(closed(&mut again).await, close::UNPAIRED);

    // A registering as a machine with what it knows takes nothing from the real one.
    let mut fake = registered(&base, A).await;
    let _b = phone(&base, B).await;
    assert!(
        matches!(frame(&mut m).await, Frame::Wopen { .. }),
        "B still reaches it"
    );
    let quiet = tokio::time::timeout(Duration::from_millis(300), fake.next()).await;
    assert!(quiet.is_err(), "nothing reaches the impostor: {quiet:?}");
}

/// A machine that never answers `wopened` does not hold a phone open forever.
#[tokio::test]
async fn a_phone_the_machine_never_opens_is_closed() {
    let base = relay(Config {
        register_timeout: Duration::from_millis(300),
        ..Config::default()
    })
    .await;
    let mut m = registered(&base, TOKEN).await;
    let mut p = phone(&base, TOKEN).await;
    let Frame::Wopen { id, .. } = frame(&mut m).await else {
        panic!()
    };
    assert_eq!(closed(&mut p).await, 1013);
    assert_eq!(
        frame(&mut m).await,
        Frame::Wclose {
            id,
            code: Some(1013),
            reason: None
        }
    );
}

#[tokio::test]
async fn one_address_holds_only_so_many_sockets_and_gets_them_back() {
    let base = relay(Config {
        limits: mooring_relay::limit::Rules {
            open: 2,
            per_minute: 0,
            trust_proxy: false,
        },
        ..Config::default()
    })
    .await;
    let mut first = registered(&base, TOKEN).await;
    let _phone = phone(&base, TOKEN).await;
    let refused = connect_async(format!("{base}/c/{TOKEN}/keepline")).await;
    let status = match refused {
        Err(tokio_tungstenite::tungstenite::Error::Http(r)) => r.status().as_u16(),
        other => panic!("a third socket was let in: {:?}", other.map(|_| ())),
    };
    assert_eq!(status, 429);
    // The machine goes; its place comes back for the next socket from the same address.
    first.close(None).await.unwrap();
    let start = Instant::now();
    loop {
        let hash = wire::token_hash("abcdefghijklmnopqrstuvwxyz999999");
        if connect_async(format!("{base}/agent?th={hash}"))
            .await
            .is_ok()
        {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "place never came back"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
