//! `mooring-relay [--listen <addr>] [--key <key>]`: runs the relay. Put it behind an
//! HTTPS proxy (Caddy, nginx) on a public host; machines and phones both dial in.
//! `--check` asks a running one on the same address for `/healthz` (the image's health
//! check); `--version` prints the version and the newest protocol it speaks.
use mooring_relay::server::VERSION;
use mooring_remote::wire::PROTOCOL;
use std::io::{self, Read, Write};

const USAGE: &str =
    "usage: mooring-relay [--listen <地址:端口>] [--key <注册密钥>] | --check | --version";

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut listen = std::env::var("RELAY_LISTEN").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let mut config = mooring_relay::server::Config {
        key: std::env::var("RELAY_REGISTRATION_KEY")
            .ok()
            .filter(|k| !k.is_empty()),
        ..Default::default()
    };
    let mut check = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| io::Error::other(format!("{arg} 需要一个值")))
        };
        match arg.as_str() {
            "--listen" => listen = value()?,
            "--key" => config.key = Some(value()?),
            "--check" => check = true,
            "--version" => {
                println!("mooring-relay {VERSION} (protocol {PROTOCOL})");
                return Ok(());
            }
            _ => return Err(io::Error::other(USAGE)),
        }
    }
    if check {
        return healthy(&listen);
    }
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    let gate = if config.key.is_some() { "key" } else { "open" };
    // Tests and scripts read the bound address from this line.
    println!(
        "mooring-relay {VERSION} (protocol {PROTOCOL}) listening on {} ({gate})",
        listener.local_addr()?
    );
    mooring_relay::server::serve(listener, config).await
}

/// `/healthz` on `listen` answers 200; a wildcard address is asked on loopback.
fn healthy(listen: &str) -> io::Result<()> {
    let port = listen.rsplit(':').next().unwrap_or("8787");
    let host = match listen.rsplit_once(':').map_or("", |(h, _)| h) {
        "" | "0.0.0.0" | "[::]" => "127.0.0.1",
        h => h,
    };
    let mut tcp = std::net::TcpStream::connect(format!("{host}:{port}"))?;
    tcp.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    write!(
        tcp,
        "GET /healthz HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )?;
    let mut reply = String::new();
    tcp.read_to_string(&mut reply)?;
    match reply.lines().next() {
        Some(status) if status.contains(" 200 ") => Ok(()),
        status => Err(io::Error::other(format!("/healthz: {status:?}"))),
    }
}
