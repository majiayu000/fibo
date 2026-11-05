use super::bridge::Bridge;
use super::live::Live;
use super::{HANDSHAKE_LIMIT, MAX_SESSIONS, PING, RECHECK, SILENCE};
use crate::remote::Access;
use futures_util::{SinkExt, StreamExt};
use mooring_core::protocol::RemoteState;
use mooring_host::devices::{self, Device, Role};
use mooring_remote::{
    close,
    secure::{self, Code},
    wire::{self, bin, Frame},
};
use serde_json::json;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

enum Ended {
    /// The relay refused this machine; redialing would not help.
    Refused(String),
    Lost(String),
}

pub(super) async fn stay_connected(
    base: &str,
    code: &Code,
    key: Option<String>,
    name: String,
    socket: &Path,
    live: &Live,
) -> io::Result<()> {
    let mut wait = Duration::from_secs(1);
    loop {
        let (ended, was_up) = connect(base, code, key.as_deref(), &name, socket, live).await;
        match ended {
            Ended::Refused(why) => return Err(io::Error::other(why)),
            Ended::Lost(why) => {
                if was_up {
                    wait = Duration::from_secs(1);
                }
                let again = format!("{why}，{} 秒后重连", wait.as_secs());
                eprintln!("mooring remote: 和中继断开：{again}");
                live.state(RemoteState::Connecting, again);
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(30));
            }
        }
    }
}

/// One connection to the relay, until it ends; says whether it got registered.
async fn connect(
    base: &str,
    code: &Code,
    key: Option<&str>,
    name: &str,
    socket: &Path,
    live: &Live,
) -> (Ended, bool) {
    let hash = wire::token_hash(&code.token());
    let url = format!("{base}{}?th={hash}", wire::AGENT_PATH);
    let ws = match tokio::time::timeout(
        Duration::from_secs(20),
        tokio_tungstenite::connect_async(&url),
    )
    .await
    {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(e)) => return (Ended::Lost(e.to_string()), false),
        Err(_) => return (Ended::Lost("连接超时".into()), false),
    };
    let (mut sink, mut stream) = ws.split();
    let mut paired = Paired::default();
    // Each phone reaches this machine by its own route; the list goes again on change.
    let mut sent = (
        devices::routes(paired.load(socket)),
        devices::revoked(socket),
    );
    let register = Frame::Register {
        routes: sent.0.clone(),
        revoked: sent.1.clone(),
        token_hash: hash,
        name: Some(name.into()),
        ver: Some(format!("mooring/{}", env!("CARGO_PKG_VERSION"))),
        key: key.map(Into::into),
        proto: Some(wire::PROTOCOL),
        retired: devices::retired(socket),
    };
    if let Err(e) = sink.send(Message::text(register.to_text())).await {
        return (Ended::Lost(e.to_string()), false);
    }
    let mut refusal = None;
    let proto = loop {
        let m = tokio::time::timeout(Duration::from_secs(10), stream.next()).await;
        match m {
            Ok(Some(Ok(Message::Text(t)))) => match serde_json::from_str::<Frame>(&t) {
                Ok(Frame::Registered { proto }) => match wire::relay_too_old(proto) {
                    Some(why) => return (Ended::Refused(why), false),
                    None => break proto.unwrap_or(1),
                },
                Ok(Frame::Error { message }) => refusal = Some(message),
                _ => (),
            },
            Ok(Some(Ok(Message::Close(f)))) => {
                let code = f.as_ref().map_or(1005, |f| u16::from(f.code));
                let why = refusal.unwrap_or_else(|| format!("中继关闭了连接（{code}）"));
                // Bad key, bad token, or full: the same again would fail the same way.
                return match code {
                    4002 | 4003 | 4401 => {
                        (Ended::Refused(format!("中继拒绝了这台机器：{why}")), false)
                    }
                    _ => (Ended::Lost(why), false),
                };
            }
            Ok(Some(Ok(_))) => (),
            Ok(Some(Err(e))) => return (Ended::Lost(e.to_string()), false),
            Ok(None) => return (Ended::Lost("中继断开".into()), false),
            Err(_) => return (Ended::Lost("中继没有回应注册".into()), false),
        }
    };
    eprintln!("mooring remote: 已连上中继 {base}");
    live.state(RemoteState::Connected, String::new());

    let (out, mut outbox) = mpsc::channel::<Message>(64);
    let writer = tokio::spawn(async move {
        let mut tick = tokio::time::interval(PING);
        tick.tick().await;
        loop {
            tokio::select! {
                m = outbox.recv() => match m {
                    Some(m) => if sink.send(m).await.is_err() { break },
                    None => break,
                },
                _ = tick.tick() => if sink.send(Message::Ping(Default::default())).await.is_err() { break },
            }
        }
    });

    let mut sessions: HashMap<String, Session> = HashMap::new();
    let log = devices::log_file(socket);
    let mut recheck = tokio::time::interval(RECHECK);
    let ended = loop {
        let deadline = tokio::time::Instant::now() + SILENCE;
        let m = tokio::select! {
            m = tokio::time::timeout_at(deadline, stream.next()) => m,
            // A phone paired or unpaired since is told to the relay; an unpaired one on
            // now is dropped within RECHECK, and one that never finishes its handshake
            // within HANDSHAKE_LIMIT.
            _ = recheck.tick() => {
                let routes = (devices::routes(paired.load(socket)), devices::revoked(socket));
                if routes != sent {
                    sent = routes;
                    let (routes, revoked) = sent.clone();
                    let _ = out.send(Message::text(Frame::Routes { routes, revoked }.to_text())).await;
                }
                let now = paired.load(socket);
                let gone: Vec<(String, Dropped)> = sessions
                    .iter()
                    .filter_map(|(id, s)| match s {
                        Session::Open { device, role, since, .. } => {
                            dropped(now, device, *role, *since).map(|why| (id.clone(), why))
                        }
                        Session::Pairing(_, at) => (at.elapsed() > HANDSHAKE_LIMIT)
                            .then(|| (id.clone(), Dropped::Handshake)),
                    })
                    .collect();
                for (id, why) in gone {
                    // Tell the phone why before the session ends and says only "done".
                    let (code, reason) = why.close();
                    close(&out, &id, code, reason).await;
                    if let Some(Session::Open { device, .. }) = sessions.remove(&id) {
                        live.left(&id);
                        eprintln!("mooring remote: 设备 {device} {}", why.label());
                        devices::log(
                            &log,
                            json!({ "event": "dropped", "device": device, "why": reason }),
                        );
                    }
                }
                continue;
            }
        };
        let m = match m {
            Err(_) => break Ended::Lost("中继很久没有消息".into()),
            Ok(None) => break Ended::Lost("中继断开".into()),
            Ok(Some(Err(e))) => break Ended::Lost(e.to_string()),
            Ok(Some(Ok(m))) => m,
        };
        // A phone's message: a binary frame, or JSON with base64 on a protocol-1 relay.
        let phone = match &m {
            Message::Binary(b) => match wire::decode(b) {
                Some((bin::WBIN, id, payload)) => Some((id.to_owned(), payload.to_vec())),
                _ => continue,
            },
            Message::Text(t) => match serde_json::from_str::<Frame>(t) {
                Ok(Frame::Wmsg { id, b: Some(b), .. }) => {
                    use base64::Engine;
                    match base64::engine::general_purpose::STANDARD.decode(b) {
                        Ok(b) => Some((id, b)),
                        Err(_) => continue,
                    }
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((id, payload)) = phone {
            let payload = &payload[..];
            let next = match sessions.remove(&id) {
                Some(Session::Pairing(device, _)) => {
                    let known = paired.load(socket).iter().find(|d| d.id == device).cloned();
                    // Disconnected from the computer: stays out for KICK_COOLDOWN_MS.
                    if known.as_ref().is_some_and(|d| d.cooling(devices::now_ms())) {
                        close(&out, &id, close::KICKED, "disconnected; try in 5 minutes").await;
                        continue;
                    }
                    let answered = known.as_ref().and_then(|d| {
                        let key = d.key()?;
                        secure::respond(&key, payload).ok().map(|r| (d, r))
                    });
                    match answered {
                        Some((d, (channel, answer))) => {
                            if out.send(to_phone(proto, &id, &answer)).await.is_err() {
                                break Ended::Lost("写不进中继".into());
                            }
                            eprintln!("mooring remote: 「{}」已连上（{}）", d.name, d.role.label());
                            devices::log(
                                &log,
                                json!({ "event": "connected", "device": d.id, "name": d.name, "role": d.role }),
                            );
                            live.joined(&id, &d.id);
                            let access = Access {
                                role: d.role,
                                device: Some((d.id.clone(), d.name.clone())),
                                log: Some(log.clone()),
                                observer: Some(live.observer(&id)),
                            };
                            Some(Session::Open {
                                since: devices::now_ms(),
                                bridge: Bridge::start(
                                    &id,
                                    channel,
                                    out.clone(),
                                    socket,
                                    proto,
                                    access,
                                ),
                                device: d.id.clone(),
                                role: d.role,
                            })
                        }
                        None => {
                            eprintln!("mooring remote: 设备 {device} 没有配对或密钥不对，已拒绝");
                            devices::log(&log, json!({ "event": "refused", "device": device }));
                            close(&out, &id, close::UNPAIRED, "not paired").await;
                            None
                        }
                    }
                }
                Some(Session::Open {
                    bridge,
                    device,
                    role,
                    since,
                }) => match bridge.receive(payload) {
                    Ok(()) => Some(Session::Open {
                        bridge,
                        device,
                        role,
                        since,
                    }),
                    Err(e) => {
                        eprintln!("mooring remote: 手机 {id} 断开：{e}");
                        live.left(&id);
                        close(&out, &id, close::BAD_MESSAGE, "bad message").await;
                        None
                    }
                },
                None => None,
            };
            if let Some(s) = next {
                sessions.insert(id, s);
            }
            continue;
        }
        match m {
            Message::Text(t) => match serde_json::from_str::<Frame>(&t) {
                Ok(Frame::Wopen { id, path }) => {
                    let ours = path.split('?').next() == Some(wire::MOORING_PATH);
                    let reply = if !ours {
                        Frame::Werr {
                            id,
                            message: "not found".into(),
                        }
                    } else if sessions.len() >= MAX_SESSIONS {
                        Frame::Werr {
                            id,
                            message: "too many phones".into(),
                        }
                    } else {
                        sessions.insert(
                            id.clone(),
                            Session::Pairing(query(&path, "d"), std::time::Instant::now()),
                        );
                        Frame::Wopened { id }
                    };
                    let _ = out.send(Message::text(reply.to_text())).await;
                }
                Ok(Frame::Wclose { id, .. }) => {
                    if let Some(Session::Open { device, .. }) = sessions.remove(&id) {
                        live.left(&id);
                        eprintln!("mooring remote: 设备 {device} 已离开");
                        devices::log(&log, json!({ "event": "left", "device": device }));
                    }
                }
                // Text messages from a phone are not part of this protocol.
                Ok(Frame::Wmsg { id, s: Some(_), .. }) => {
                    sessions.remove(&id);
                    close(&out, &id, close::BAD_MESSAGE, "binary only").await;
                }
                Ok(Frame::Hreq { id }) => {
                    let head = Frame::Hres {
                        id: id.clone(),
                        status: 404,
                        headers: Default::default(),
                    };
                    let _ = out.send(Message::text(head.to_text())).await;
                    let _ = out.send(Message::text(Frame::Hend { id }.to_text())).await;
                }
                _ => (),
            },
            Message::Close(f) => {
                let code = f.as_ref().map_or(1005, |f| u16::from(f.code));
                break match code {
                    4000 => {
                        Ended::Refused("另一个 mooring 用同一个配对码连上了中继，这一个退出".into())
                    }
                    _ => Ended::Lost(format!("中继关闭了连接（{code}）")),
                };
            }
            _ => (),
        }
    };
    // Dropping a bridge ends its session.
    drop(sessions);
    drop(out);
    writer.abort();
    (ended, true)
}

/// A message to one phone, in the form the relay speaks.
pub(super) fn to_phone(proto: u32, id: &str, bytes: &[u8]) -> Message {
    if proto >= 2 {
        return Message::binary(wire::encode(bin::WBIN, id, bytes));
    }
    use base64::Engine;
    let b = base64::engine::general_purpose::STANDARD.encode(bytes);
    Message::text(
        Frame::Wmsg {
            id: id.into(),
            s: None,
            b: Some(b),
        }
        .to_text(),
    )
}

async fn close(out: &mpsc::Sender<Message>, id: &str, code: u16, reason: &str) {
    let frame = Frame::Wclose {
        id: id.into(),
        code: Some(code),
        reason: Some(reason.into()),
    };
    let _ = out.send(Message::text(frame.to_text())).await;
}

enum Session {
    /// Opened by the device the path names; its first message is its half of the handshake.
    /// When it opened: one that never finishes is closed after `HANDSHAKE_LIMIT`.
    Pairing(String, std::time::Instant),
    Open {
        bridge: Bridge,
        device: String,
        /// The role it connected with; a change drops the session.
        role: Role,
        /// Unix milliseconds; a "disconnect now" after it drops the session.
        since: u64,
    },
}

/// Why an open session ends before the phone leaves; each has its own close code, since
/// the phone does something different for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dropped {
    /// No longer paired: only a new pairing helps.
    Unpaired,
    /// "Disconnect" on the computer: still paired, may come back.
    Kicked,
    /// A new role: connecting again gives it.
    RoleChanged,
    /// Opened but never finished its handshake within HANDSHAKE_LIMIT.
    Handshake,
}

impl Dropped {
    fn close(self) -> (u16, &'static str) {
        match self {
            Dropped::Unpaired => (close::UNPAIRED, "unpaired"),
            Dropped::Kicked => (close::KICKED, "disconnected"),
            Dropped::RoleChanged => (close::ROLE_CHANGED, "role changed"),
            // A plain close (the relay passes only 1000 and 3000–4999 on): a real phone
            // that was merely slow retries later.
            Dropped::Handshake => (1000, "handshake timeout"),
        }
    }
    fn label(self) -> &'static str {
        match self {
            Dropped::Unpaired => "被取消配对",
            Dropped::Kicked => "被电脑断开",
            Dropped::RoleChanged => "改了权限，断开等它重连",
            Dropped::Handshake => "没在时限内完成握手，已断开",
        }
    }
}

/// Whether a session the device opened at `since` (ms) with `role` must end now.
fn dropped(now: &[Device], device: &str, role: Role, since: u64) -> Option<Dropped> {
    let Some(d) = now.iter().find(|d| d.id == device) else {
        return Some(Dropped::Unpaired);
    };
    if d.kicked >= since {
        Some(Dropped::Kicked)
    } else if d.role != role {
        Some(Dropped::RoleChanged)
    } else {
        None
    }
}

/// The paired devices, read again only when their file changed.
#[derive(Default)]
struct Paired {
    stamp: Option<(std::time::SystemTime, u64)>,
    devices: Vec<Device>,
}
impl Paired {
    fn load(&mut self, socket: &Path) -> &[Device] {
        let stamp = std::fs::metadata(socket.with_file_name("remote-devices.json"))
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        if stamp.is_none() || stamp != self.stamp {
            self.devices = devices::load(socket);
            self.stamp = stamp;
        }
        &self.devices
    }
}

/// `name`'s value in a path's query, or empty.
fn query(path: &str, name: &str) -> String {
    path.split_once('?')
        .map(|(_, q)| q)
        .unwrap_or_default()
        .split('&')
        .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
