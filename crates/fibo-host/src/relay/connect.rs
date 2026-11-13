use super::bridge::Bridge;
use super::live::Live;
use super::{HANDSHAKE_LIMIT, MAX_SESSIONS, PING, RECHECK, SILENCE};
use crate::remote::Access;
use fibo_core::protocol::RemoteState;
use fibo_host::devices::{self, Device, Role};
use fibo_remote::{
    close,
    secure::{self, Code},
    wire::{self, bin, Frame},
};
use futures_util::{SinkExt, StreamExt};
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
    /// Another connection registered with this machine's code (4000). It may be another
    /// computer given the same code, or someone who read the code's hash off a proxy log
    /// (#69): either way leaving for good hands the code over, so it tries again later.
    Replaced,
    Lost(String),
}

/// How long a machine pushed off by another connection on its code waits before it takes
/// the code back. Long, so two computers sharing a code take turns for minutes at a time
/// instead of throwing each other off every second.
pub(super) const REPLACED_WAIT: Duration = Duration::from_secs(5 * 60);

/// Which process on this machine last dialed the relay for this runtime. Written before each
/// dial, so when the relay pushes one off (4000) it can tell a newer process here (a host
/// restarted beside an old child, say) from a connection elsewhere: the first it leaves to,
/// as it always did, the second it takes the code back from later.
fn owner_file(socket: &Path) -> std::path::PathBuf {
    socket.with_file_name("remote-relay.owner")
}

fn claim(socket: &Path) {
    let _ = std::fs::write(owner_file(socket), std::process::id().to_string());
}

/// Whether another live process on this machine dialed after this one. A build before 11-09
/// writes no owner: replaced by one, this process takes the code back in [`REPLACED_WAIT`]
/// and that build, on its 4000, leaves for good.
pub(super) fn taken_here(socket: &Path) -> bool {
    std::fs::read_to_string(owner_file(socket))
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok())
        .is_some_and(|pid| pid != std::process::id() && fibo_platform::process::exists(pid as i32))
}

/// What follows a connection that ended: give up, or what to say and how long to wait
/// before the next one. `wait` is the backoff after a lost connection; it starts over once
/// one got registered.
fn after(ended: Ended, was_up: bool, wait: &mut Duration) -> io::Result<(Duration, String)> {
    match ended {
        Ended::Refused(why) => Err(io::Error::other(why)),
        Ended::Replaced => {
            *wait = Duration::from_secs(1);
            let why = format!(
                "另一个连接用这台电脑的配对码连上了中继，把它顶了下来（同一个码在另一台电脑上，或码被别人拿到了），{} 分钟后再连",
                REPLACED_WAIT.as_secs() / 60
            );
            Ok((REPLACED_WAIT, why))
        }
        Ended::Lost(why) => {
            if was_up {
                *wait = Duration::from_secs(1);
            }
            let now = *wait;
            *wait = (now * 2).min(Duration::from_secs(30));
            Ok((now, format!("{why}，{} 秒后重连", now.as_secs())))
        }
    }
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
        let (pause, again) = after(ended, was_up, &mut wait)?;
        eprintln!("fibo remote: 和中继断开：{again:?}");
        live.state(RemoteState::Connecting, again);
        tokio::time::sleep(pause).await;
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
    // Nothing secret in the URL: proxies log it, and the hash is all it takes to register as
    // this machine (#69). The relay never read a query here; the hash goes in `Register`.
    let url = format!("{base}{}", wire::AGENT_PATH);
    claim(socket);
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
        ver: Some(format!("fibo/{}", env!("CARGO_PKG_VERSION"))),
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
                Ok(Frame::Error { message }) => refusal = Some(format!("{message:?}")),
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
    eprintln!("fibo remote: 已连上中继 {base:?}");
    live.state(RemoteState::Connected, String::new());

    let (out, mut outbox) = mpsc::channel::<Message>(64);
    let mut writer = tokio::spawn(async move {
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
    let mut deadline = tokio::time::Instant::now() + SILENCE;
    let ended = loop {
        let m = tokio::select! {
            m = tokio::time::timeout_at(deadline, stream.next()) => m,
            _ = &mut writer => break Ended::Lost("中继发送连接已结束".into()),
            // A phone paired or unpaired since is told to the relay; an unpaired one on
            // now is dropped within RECHECK, and one that never finishes its handshake
            // within HANDSHAKE_LIMIT, including proof of the channel key afterward.
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
                        Session::Open { device, role, since, first_message, .. } => {
                            dropped(now, device, *role, *since)
                                .or_else(|| first_message.filter(|at| tokio::time::Instant::now() >= *at)
                                    .map(|_| Dropped::Handshake))
                                .map(|why| (id.clone(), why))
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
                        eprintln!("fibo remote: 设备 {device:?} {}", why.label());
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
        deadline = tokio::time::Instant::now() + SILENCE;
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
                            let access = Access {
                                role: d.role,
                                device: Some((d.id.clone(), d.name.clone())),
                                log: Some(log.clone()),
                                observer: Some(live.observer(&id)),
                            };
                            let bridge = match Bridge::start(
                                &id,
                                channel,
                                out.clone(),
                                socket,
                                proto,
                                access,
                            ) {
                                Ok(bridge) => bridge,
                                Err(e) => {
                                    eprintln!("fibo remote: 手机 {id:?} 无法启动会话：{e:?}");
                                    close(&out, &id, close::NORMAL, "session unavailable").await;
                                    continue;
                                }
                            };
                            if out.send(to_phone(proto, &id, &answer)).await.is_err() {
                                break Ended::Lost("写不进中继".into());
                            }
                            eprintln!("fibo remote: 「{:?}」已连上（{}）", d.name, d.role.label());
                            devices::log(
                                &log,
                                json!({ "event": "connected", "device": d.id, "name": d.name, "role": d.role }),
                            );
                            live.joined(&id, &d.id);
                            Some(Session::Open {
                                since: devices::now_ms(),
                                first_message: Some(tokio::time::Instant::now() + HANDSHAKE_LIMIT),
                                bridge,
                                device: d.id.clone(),
                                role: d.role,
                            })
                        }
                        None => {
                            eprintln!("fibo remote: 设备 {device:?} 没有配对或密钥不对，已拒绝");
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
                    ..
                }) => match bridge.receive(payload) {
                    Ok(()) => Some(Session::Open {
                        bridge,
                        device,
                        role,
                        since,
                        first_message: None,
                    }),
                    Err(e) => {
                        eprintln!("fibo remote: 手机 {id:?} 断开：{e:?}");
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
                    // The relay picks the id. Every message to this phone carries it in a
                    // binary frame, so one that does not fit never becomes a session.
                    let reply = if id.len() > wire::MAX_ID_LEN {
                        Frame::Werr {
                            id,
                            message: "id too long".into(),
                        }
                    } else if !ours {
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
                            Session::Pairing(query(&path, "d"), tokio::time::Instant::now()),
                        );
                        Frame::Wopened { id }
                    };
                    let _ = out.send(Message::text(reply.to_text())).await;
                }
                Ok(Frame::Wclose { id, .. }) => {
                    if let Some(Session::Open { device, .. }) = sessions.remove(&id) {
                        live.left(&id);
                        eprintln!("fibo remote: 设备 {device:?} 已离开");
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
                    4000 if taken_here(socket) => {
                        Ended::Refused("这台电脑上另一个连接中继的进程接管了，这一个退出".into())
                    }
                    4000 => Ended::Replaced,
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
    Pairing(String, tokio::time::Instant),
    Open {
        bridge: Bridge,
        device: String,
        /// The role it connected with; a change drops the session.
        role: Role,
        /// Unix milliseconds; a "disconnect now" after it drops the session.
        since: u64,
        /// A replayed handshake cannot prove the channel key; require its first message.
        first_message: Option<tokio::time::Instant>,
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
