use super::*;

/// A phone on the other side of a relay: the Noise channel, acknowledgements, and the
/// remote protocol's lines. `ack: false` plays a phone that stopped reading.
pub(crate) struct Phone {
    rt: tokio::runtime::Runtime,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    channel: fibo_remote::secure::Channel,
    lines: Vec<u8>,
    pub(crate) ack: bool,
    /// DATA bytes received, acknowledged or not.
    pub(crate) received: u64,
}
impl Phone {
    pub(crate) fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }
    /// Retries while the machine is not on the relay yet.
    pub(crate) fn connect(link: &fibo_remote::link::Link) -> Self {
        Self::try_connect(link).expect("handshake")
    }
    /// The handshake's answer, or the close code the machine answered with instead.
    pub(crate) fn try_connect(link: &fibo_remote::link::Link) -> Result<Self, Option<u16>> {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        let rt = Self::rt();
        let url = link.url().unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        let mut ws = loop {
            match rt.block_on(tokio_tungstenite::connect_async(&url)) {
                Ok((ws, _)) => break ws,
                Err(e) => {
                    assert!(
                        Instant::now() < until,
                        "phone never reached the machine: {e}"
                    );
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };
        let (hs, first) = fibo_remote::secure::initiate(&link.key).unwrap();
        let channel = rt.block_on(async {
            ws.send(Message::binary(first)).await.unwrap();
            loop {
                match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
                    Ok(Some(Ok(Message::Binary(b)))) => return Ok(hs.finish(&b).unwrap()),
                    Ok(Some(Ok(Message::Close(f)))) => return Err(f.map(|f| u16::from(f.code))),
                    Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
                    other => panic!("no handshake answer: {other:?}"),
                }
            }
        })?;
        Ok(Self {
            rt,
            ws,
            channel,
            lines: vec![],
            ack: true,
            received: 0,
        })
    }
    /// Reads until the machine closes the socket; its close code.
    pub(crate) fn closed(&mut self, wait: Duration) -> Option<u16> {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;
        let ws = &mut self.ws;
        self.rt.block_on(async move {
            tokio::time::timeout(wait, async {
                loop {
                    match ws.next().await {
                        Some(Ok(Message::Close(f))) => return f.map(|f| u16::from(f.code)),
                        Some(Ok(_)) => continue,
                        _ => return None,
                    }
                }
            })
            .await
            .ok()
            .flatten()
        })
    }
    pub(crate) fn say(&mut self, v: serde_json::Value) {
        use futures_util::SinkExt;
        let line = format!("{v}\n");
        for chunk in line.as_bytes().chunks(fibo_remote::secure::CHUNK) {
            let sealed = self.channel.data(chunk).unwrap();
            self.rt
                .block_on(
                    self.ws
                        .send(tokio_tungstenite::tungstenite::Message::binary(sealed)),
                )
                .unwrap();
        }
    }
    /// The next line, or `None` after `wait` without one.
    pub(crate) fn line(&mut self, wait: Duration) -> Option<serde_json::Value> {
        use fibo_remote::secure::Message as Secure;
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        let until = Instant::now() + wait;
        loop {
            if let Some(end) = self.lines.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.lines.drain(..=end).collect();
                let v: serde_json::Value = serde_json::from_slice(&line).unwrap();
                // Every line the machine sends fits the protocol's types.
                serde_json::from_value::<fibo_remote::protocol::Incoming>(v.clone())
                    .unwrap_or_else(|e| panic!("{v} does not fit the protocol types: {e}"));
                return Some(v);
            }
            let left = until.checked_duration_since(Instant::now())?;
            let ws = &mut self.ws;
            let m = self
                .rt
                .block_on(async move { tokio::time::timeout(left, ws.next()).await })
                .ok()?;
            match m {
                Some(Ok(Message::Binary(b))) => match self.channel.open(&b).unwrap() {
                    Secure::Data(d) => {
                        self.received += d.len() as u64;
                        if self.ack {
                            let ack = self.channel.ack(d.len() as u32).unwrap();
                            self.rt
                                .block_on(self.ws.send(Message::binary(ack)))
                                .unwrap();
                        }
                        self.lines.extend(d);
                    }
                    Secure::Ack(_) => (),
                },
                Some(Ok(Message::Close(f))) => panic!("closed: {f:?}"),
                Some(Ok(_)) => (),
                other => panic!("socket ended: {other:?}"),
            }
        }
    }
    pub(crate) fn next(&mut self, pick: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let v = self.line(left).expect("no message");
            if pick(&v) {
                return v;
            }
        }
    }
    /// Takes every acknowledgement it held back, as a phone that reads again would.
    pub(crate) fn resume(&mut self) {
        use futures_util::SinkExt;
        self.ack = true;
        let ack = self
            .channel
            .ack(self.received.min(u32::MAX as u64) as u32)
            .unwrap();
        self.rt
            .block_on(
                self.ws
                    .send(tokio_tungstenite::tungstenite::Message::binary(ack)),
            )
            .unwrap();
    }
}

/// Pairs one phone and reads back the link `fibo remote --pair` printed.
pub(crate) fn pair(root: &str, base: &str, name: &str, role: &str) -> fibo_remote::link::Link {
    let out = Command::new(env!("CARGO_BIN_EXE_fibo"))
        .args([
            "remote", "--pair", "--relay", base, "--name", name, "--role", role,
        ])
        .args(["--runtime", root])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let printed = String::from_utf8(out.stdout).unwrap();
    assert!(printed.contains('▀') || printed.contains('█'), "no QR code");
    let line = printed
        .lines()
        .find(|l| l.starts_with("mooring://pair?"))
        .expect("pairing link");
    fibo_remote::link::Link::parse(line).unwrap()
}

/// A relay on a free local port, until the returned sender is dropped.
pub(crate) fn local_relay(
    addr: &str,
    config: fibo_relay::server::Config,
) -> (
    String,
    std::sync::mpsc::Sender<()>,
    std::thread::JoinHandle<()>,
) {
    let (ready, bound) = std::sync::mpsc::channel();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let addr = addr.to_owned();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            ready.send(listener.local_addr().unwrap()).unwrap();
            let serve = fibo_relay::server::serve(listener, config);
            let stop = tokio::task::spawn_blocking(move || {
                let _ = stopped.recv();
            });
            tokio::select! {
                _ = serve => (),
                _ = stop => (),
            }
        });
    });
    let addr = bound.recv().unwrap();
    (format!("ws://{addr}"), stop, thread)
}
