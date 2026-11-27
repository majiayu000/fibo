//! `fibo remote dial --machine <名字> (--listen <socket> | --about)`: the computer that
//! reaches another through the relay. `--listen` keeps a local socket that stands for that
//! machine's own, as `ssh -L` does: each connection to it is one session through the relay,
//! a Noise channel the relay cannot read, to that machine's native protocol. `--about`
//! prints what that machine says it is ([`About`]) and ends.
//!
//! Flow control is the phone's: at most [`secure::WINDOW`] bytes sent and not yet
//! acknowledged each way; what arrives is acknowledged once it was written here. After
//! [`About`] both ways are compressed (`squeeze.rs`), and windows count compressed bytes.
use super::computer::{load_link, About};
use super::squeeze::{Squeeze, Unsqueeze};
use fibo_platform::local;
use fibo_remote::{
    link::Link,
    secure::{self, Channel, Message as Secure},
};
use futures_util::{SinkExt, StreamExt};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const PING: Duration = Duration::from_secs(25);

pub(super) fn run(machine: &str, listen: Option<&Path>) -> io::Result<()> {
    let link = load_link(machine)?;
    let Some(listen) = listen else {
        let rt = runtime()?;
        let (_, _, about) = rt.block_on(open(&link))?;
        println!("{}", serde_json::to_string(&about)?);
        return Ok(());
    };
    // Ends with the window that started it, however it went (a crash, a TERM): left
    // behind, it would keep its sessions open on the relay and on that machine.
    fibo_platform::process::exit_with_parent();
    // As `ssh -o StreamLocalBindUnlink=yes`: a socket left by an earlier run is replaced.
    let _ = std::fs::remove_file(listen);
    let listener = local::Listener::bind(listen)?;
    let _ = local::restrict(listen);
    loop {
        let stream = listener.accept()?;
        let link = link.clone();
        std::thread::spawn(move || {
            if let Err(e) = runtime().and_then(|rt| rt.block_on(carry(&link, stream))) {
                eprintln!("fibo remote dial: {e}");
            }
        });
    }
}

fn runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// One session through the first relay of the link that answers: the channel, and what
/// the machine said it is.
async fn open(link: &Link) -> io::Result<(Ws, Channel, About)> {
    let mut why = io::Error::other(t!("配对链接里没有中继"));
    for relay in std::iter::once(&link.relay).chain(&link.alts) {
        let link = Link {
            relay: relay.clone(),
            ..link.clone()
        };
        match tokio::time::timeout(Duration::from_secs(20), open_on(&link)).await {
            Ok(Ok(open)) => return Ok(open),
            Ok(Err(e)) => why = e,
            Err(_) => why = io::Error::other(t!("{relay} 连接超时", relay)),
        }
    }
    Err(why)
}

async fn open_on(link: &Link) -> io::Result<(Ws, Channel, About)> {
    let url = link.url().map_err(io::Error::other)?;
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| io::Error::other(t!("连不上中继 {relay}：{e}", relay = &link.relay, e)))?;
    let (handshake, first) = secure::initiate(&link.key)?;
    let mut handshake = Some(handshake);
    ws.send(Message::binary(first))
        .await
        .map_err(io::Error::other)?;
    let mut channel = None;
    loop {
        // A machine that answered the handshake but says nothing serves phones only.
        let m = match channel {
            None => ws.next().await,
            Some(_) => tokio::time::timeout(Duration::from_secs(5), ws.next())
                .await
                .map_err(|_| {
                    io::Error::other(t!(
                        "{name} 没有按电脑会话回话：那台的 Fibo 太旧，先在那台升级",
                        name = &link.name
                    ))
                })?,
        };
        let bytes = match m {
            Some(Ok(Message::Binary(b))) => b,
            Some(Ok(Message::Close(f))) => {
                let (code, reason) = f.map_or((1005, String::new()), |f| {
                    (u16::from(f.code), f.reason.to_string())
                });
                return Err(io::Error::other(t!(
                    "{name} 拒绝了这台电脑（{code} {reason}）：可能已取消配对，或不是完全控制",
                    name = &link.name,
                    code,
                    reason
                )));
            }
            Some(Ok(_)) => continue,
            Some(Err(e)) => return Err(io::Error::other(e)),
            None => return Err(io::Error::other(t!("中继断开"))),
        };
        let Some(c) = channel.as_mut() else {
            let handshake = handshake.take().expect("one answer");
            channel = Some(handshake.finish(&bytes)?);
            continue;
        };
        // The first message after the handshake says where this arrived.
        let Secure::Data(line) = c.open(&bytes)? else {
            continue;
        };
        let about: About = serde_json::from_slice(&line).map_err(|e| {
            io::Error::other(t!(
                "{name} 回的不是电脑会话（{e}）：那台的 Fibo 太旧，或这条配对是手机的",
                name = &link.name,
                e
            ))
        })?;
        let ack = c.ack(line.len() as u32)?;
        ws.send(Message::binary(ack))
            .await
            .map_err(io::Error::other)?;
        return Ok((ws, channel.expect("set above"), about));
    }
}

/// Carries one local connection through its own session until either end closes.
async fn carry(link: &Link, stream: local::Stream) -> io::Result<()> {
    let (mut ws, mut channel, _) = open(link).await?;
    // Read here on a thread: what the window sends.
    let (sent, mut from_window) = mpsc::channel::<Vec<u8>>(8);
    let mut reader = stream.try_clone()?;
    std::thread::spawn(move || {
        let mut buf = vec![0; secure::CHUNK];
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            if sent.blocking_send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    // Written here on a thread: what the machine sends; each write comes back to be
    // acknowledged.
    let (to_window, inbox) = std::sync::mpsc::channel::<(Vec<u8>, u32)>();
    let (written, mut done) = mpsc::unbounded_channel::<u32>();
    let mut writer = stream.try_clone()?;
    std::thread::spawn(move || {
        for (bytes, received) in inbox {
            if writer.write_all(&bytes).is_err() {
                break;
            }
            if written.send(received).is_err() {
                break;
            }
        }
        let _ = writer.shutdown(std::net::Shutdown::Write);
    });
    let (mut squeeze, mut unsqueeze) = (Squeeze::new()?, Unsqueeze::new()?);
    let mut waiting: VecDeque<Vec<u8>> = VecDeque::new();
    let mut in_flight: u64 = 0;
    let mut window_open = true;
    let mut ping = tokio::time::interval(PING);
    ping.tick().await;
    let ended = loop {
        // Sends what the window wrote while the machine's window has room.
        while let Some(next) = waiting.front() {
            if in_flight + next.len() as u64 > secure::WINDOW {
                break;
            }
            let next = waiting.pop_front().expect("front");
            in_flight += next.len() as u64;
            let sealed = channel.data(&next)?;
            ws.send(Message::binary(sealed))
                .await
                .map_err(io::Error::other)?;
        }
        let room = waiting.is_empty() && window_open;
        tokio::select! {
            bytes = from_window.recv(), if room => match bytes {
                // Each message within `secure::CHUNK`, compressed or not.
                Some(bytes) => waiting.extend(squeeze.pack(&bytes)?.chunks(secure::CHUNK).map(<[u8]>::to_vec)),
                None => window_open = false,
            },
            Some(n) = done.recv() => {
                let ack = channel.ack(n)?;
                ws.send(Message::binary(ack)).await.map_err(io::Error::other)?;
            }
            m = ws.next() => match m {
                Some(Ok(Message::Binary(b))) => match channel.open(&b)? {
                    Secure::Data(bytes) => {
                        let received = bytes.len() as u32;
                        if to_window.send((unsqueeze.unpack(&bytes)?, received)).is_err() {
                            break Ok(());
                        }
                    }
                    Secure::Ack(n) => in_flight = in_flight.saturating_sub(n.into()),
                },
                Some(Ok(Message::Close(_))) | None => break Ok(()),
                Some(Ok(_)) => (),
                Some(Err(e)) => break Err(io::Error::other(e)),
            },
            _ = ping.tick() => {
                ws.send(Message::Ping(Default::default())).await.map_err(io::Error::other)?;
            }
        }
        if !window_open && waiting.is_empty() {
            break Ok(());
        }
    };
    let _ = ws.close(None).await;
    let _ = stream.shutdown(std::net::Shutdown::Both);
    ended
}
