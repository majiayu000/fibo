//! `mooring remote --relay <url>`: this machine on a relay, so a paired phone reaches
//! it from anywhere. The machine dials out and keeps one socket to the relay; each phone
//! that opens `/c/<token>/mooring` is one remote-protocol session, the same as
//! `remote --stdio`, run inside a Noise channel the relay cannot read.
//!
//! Next to the socket, readable only by this user: the machine's `remote-code`, which it
//! registers with and no pairing link carries; per phone, in `remote-devices.json`, its
//! own route code (its link carries it) and a key its handshake proves, deciding what it
//! may do. Only phones whose route hashes this machine lists reach it (`devices::routes`,
//! resent as `Frame::Routes` on pair and unpair); unpairing drops their open sessions.
//! `--pair --relay <url>` pairs a phone and prints its link and QR code; `--devices`
//! lists them, `--revoke` unpairs one, `--new-code` replaces the code and unpairs all.
use crate::control::Args;
use mooring_core::protocol::{rpc, RemoteChange, RemoteState, Reply, Request};
use mooring_host::devices::{self, Role};
use mooring_remote::link::Link;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

mod bridge;
mod connect;
mod live;
mod status_text;

use connect::stay_connected;
use live::Live;

/// Phones one machine serves at once.
const MAX_SESSIONS: usize = 64;
/// Silence from the relay after which the socket counts as dead; it pings every 25 s.
const SILENCE: Duration = Duration::from_secs(75);
const PING: Duration = Duration::from_secs(30);
/// Phone input waiting for the session; a phone that floods past it is closed.
const INPUT_QUEUE: usize = 256;

/// How often open sessions are checked against the paired devices, so an unpaired
/// phone is dropped within this long.
const RECHECK: Duration = Duration::from_secs(1);
/// How long an opened phone may take to finish its handshake. Anyone who has seen the
/// code can open sessions; without a limit a few idle ones would fill MAX_SESSIONS.
const HANDSHAKE_LIMIT: Duration = Duration::from_secs(10);

pub fn run(a: &Args) -> io::Result<()> {
    if a.has("devices") {
        return list_devices(&a.socket);
    }
    let root = a.socket.parent().unwrap_or(Path::new("."));
    if a.has("enable") || a.has("disable") {
        let kept = mooring_host::remote_config(root);
        let relay = a.value("relay").map_or(kept.relay, str::to_owned);
        let key = a.value("relay-key").map(str::to_owned).or(kept.key);
        let enabled = a.has("enable");
        if enabled && relay.is_empty() {
            return Err(io::Error::other("--enable 需要 --relay <中继地址>"));
        }
        host(
            a,
            Request::RemoteConfigure {
                enabled,
                relay: relay.clone(),
                key,
            },
        )?;
        println!(
            "{}",
            if enabled {
                format!("已打开：后台会一直连着 {relay}，配对过的手机随时能连过来")
            } else {
                "已关闭：后台断开中继，配对还在".to_owned()
            }
        );
        return Ok(());
    }
    if a.has("status") {
        let Reply::Recognition(status) = host(a, Request::RecognitionStatus)? else {
            return Err(io::Error::other("unexpected reply"));
        };
        if a.has("json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&status.remote).map_err(io::Error::other)?
            );
        } else {
            print!("{}", status_text::render(&status.remote, devices::now()));
        }
        return Ok(());
    }
    if let Some(name) = a.value("machine-name") {
        let m = devices::set_machine_name(&a.socket, name)?;
        println!("手机上这台电脑叫「{}」，手机重连后显示", m.name);
        return Ok(());
    }
    if let Some(needle) = a.value("kick") {
        host(
            a,
            Request::RemoteDevice {
                device: needle.to_owned(),
                change: RemoteChange::Kick,
            },
        )?;
        println!("已断开 {needle} 正连着的会话；它还配对着，可以再连");
        return Ok(());
    }
    if let (Some(needle), Some(role)) = (a.value("device"), a.value("role")) {
        let role = devices::parse_role(role)?;
        let d = devices::set_role(&a.socket, needle, role)?;
        refresh(a);
        println!(
            "{}（{}）的权限改成「{}」，它正连着的会话一秒内断开，重连后按新权限",
            d.name,
            d.id,
            d.role.label()
        );
        return Ok(());
    }
    if let Some(needle) = a.value("revoke") {
        let gone = devices::remove(&a.socket, needle)?;
        refresh(a);
        println!(
            "已取消配对 {}（{}），它正连着的会话一秒内断开",
            gone.name, gone.id
        );
        return Ok(());
    }
    let relay = a
        .value("relay")
        .ok_or_else(|| io::Error::other("--relay 需要中继地址，例如 https://relay.example.com"))?;
    let base = Link::websocket_base(relay).map_err(io::Error::other)?;
    let new = a.has("new-code");
    let code = devices::machine_code(&a.socket, new)?;
    if new {
        devices::clear(&a.socket)?;
    }
    if a.has("pair") {
        let role = a
            .value("role")
            .map_or(Ok(Role::Full), devices::parse_role)?;
        return pair(&a.socket, relay, a.value("name").unwrap_or("手机"), role);
    }
    let key = a
        .value("relay-key")
        .map(str::to_owned)
        .or_else(|| std::env::var("MOORING_RELAY_KEY").ok())
        .filter(|k| !k.is_empty());
    let name = devices::machine(&a.socket)?.name;
    let live = Live::new(a.has("report").then(|| a.socket.clone()));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let ended = runtime.block_on(stay_connected(&base, &code, key, name, &a.socket, &live));
    if let Err(e) = &ended {
        live.state(RemoteState::Failed, e.to_string());
        live.flush();
    }
    ended
}

/// Lets a running host show the app the devices as they are now; none running is fine.
fn refresh(a: &Args) {
    let _ = rpc(
        &a.socket,
        &Request::RemoteDevice {
            device: String::new(),
            change: RemoteChange::Refresh,
        },
    );
}

/// Asks the running host.
fn host(a: &Args, request: Request) -> io::Result<Reply> {
    match rpc(&a.socket, &request)? {
        Reply::Error(e) => Err(io::Error::other(e)),
        reply => Ok(reply),
    }
}

fn pair(socket: &Path, relay: &str, name: &str, role: Role) -> io::Result<()> {
    let (device, link) = devices::pair_link(socket, relay, name, role)?;
    let qr = qrcode::QrCode::new(link.as_bytes())
        .map_err(|e| io::Error::other(e.to_string()))?
        .render::<qrcode::render::unicode::Dense1x2>()
        .dark_color(qrcode::render::unicode::Dense1x2::Light)
        .light_color(qrcode::render::unicode::Dense1x2::Dark)
        .quiet_zone(true)
        .build();
    let mut out = io::stdout().lock();
    writeln!(out, "{qr}")?;
    writeln!(
        out,
        "「{name}」（{}，权限：{}）：用手机 App 扫码配对，或者粘贴这一行：",
        device.id,
        role.label()
    )?;
    writeln!(out, "{link}")?;
    writeln!(
        out,
        "这一行只给这一台手机用，别发给别人。取消它：fibo remote --revoke {}",
        device.id
    )
}

fn list_devices(socket: &Path) -> io::Result<()> {
    let list = devices::load(socket);
    if list.is_empty() {
        println!("还没有配对的设备：fibo remote --pair --relay <中继地址> --name <名字>");
        return Ok(());
    }
    let seen = devices::last_seen(socket);
    let ago = |at: u64| mooring_core::time::ago(at, devices::now());
    for d in list {
        let last = seen.get(&d.id).map_or("没连过".to_owned(), |&at| ago(at));
        println!(
            "{}  {}  {}  配对于 {}  最近连接 {}",
            d.id,
            d.name,
            d.role.label(),
            ago(d.paired),
            last
        );
    }
    Ok(())
}
