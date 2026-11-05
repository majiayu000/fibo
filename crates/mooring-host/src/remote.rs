//! `mooring remote --stdio`: this machine's terminals for a client elsewhere, a phone
//! app or another machine's workbench, one JSON object per line each way. Over ssh today
//! (`ssh host mooring remote --stdio`); a later transport carries the same lines.
//! Requests run one after another in the order sent, so text then Enter arrive in that
//! order; `events` and `watch` are streams that run beside them until closed.
//! The message shapes are in the Keepline repo's `specs/extensions/remote-protocol.md`
//! (not moved here).
use crate::control::{self, Args};
use base64::Engine;
use mooring_core::{model::*, protocol::*, terminal::Vt};
use mooring_host::devices::{self, Role};
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

mod asks;
mod chat;
mod diff;
mod dirs;
mod list;
mod quota;
mod resume;
mod tools;
mod usage;
mod writes;

const MAX_LINE: u64 = rp::MAX_LINE as u64;
use rp::OPS;
/// A client that stops reading holds up its own streams, never the host: the terminal
/// marks a stalled viewer lagged and sends it the screen again once it reads.
const OUT_QUEUE: usize = 256;

type Out = mpsc::SyncSender<Value>;
/// Open streams by the id of the request that opened them, to close one on request.
type Streams = Arc<Mutex<Registry>>;
#[derive(Default)]
struct Registry {
    /// `None`: closed by the client before it had opened.
    open: HashMap<u64, Option<UnixStream>>,
    /// Streams that took their terminal over (`watch` with `take`): `resize` sizes them.
    writers: std::collections::HashSet<u64>,
    /// The client has gone; a stream still connecting must not open.
    ended: bool,
}

/// Who is on the other end and what it may do. Over ssh it is this user, already trusted.
#[derive(Clone)]
pub(crate) struct Access {
    pub(crate) role: Role,
    /// `(id, name)` of a paired device.
    pub(crate) device: Option<(String, String)>,
    /// Where acting ops are logged.
    pub(crate) log: Option<PathBuf>,
    /// Told what the client watches and does, for the app to show who is on.
    pub(crate) observer: Option<Observer>,
}
pub(crate) type Observer = Arc<dyn Fn(Activity) + Send + Sync>;
pub(crate) enum Activity {
    Watching(String),
    Unwatched(String),
    Acted {
        op: String,
        terminal: Option<String>,
    },
}
impl Access {
    fn trusted() -> Self {
        Self {
            role: Role::Full,
            device: None,
            log: None,
            observer: None,
        }
    }
    fn tell(&self, activity: Activity) {
        if let Some(o) = &self.observer {
            o(activity);
        }
    }
    fn record(&self, op: &str, request: &Value, result: &io::Result<Value>) {
        let (Some(log), Some((id, name))) = (&self.log, &self.device) else {
            return;
        };
        devices::log(
            log,
            json!({
                "event": "op",
                "device": id,
                "name": name,
                "op": op,
                "terminal": request["terminal"],
                "error": result.as_ref().err().map(|e| e.to_string()),
            }),
        );
    }
}

/// A host's relay connection ends with its host, however the host went: handing over to
/// a newer one exits without unwinding, and a crash signals nobody. Left behind, it would
/// redial and take the machine's place on the relay from the new host's connection.
fn exit_with_parent() {
    // SAFETY: getppid(2) takes no arguments and cannot fail.
    let parent = unsafe { libc::getppid() };
    thread::spawn(move || loop {
        thread::sleep(std::time::Duration::from_secs(1));
        // SAFETY: as above.
        if unsafe { libc::getppid() } != parent {
            std::process::exit(0);
        }
    });
}

pub fn run(args: impl Iterator<Item = String>) -> io::Result<()> {
    let cmd = control::command("remote").expect("remote is in the command table");
    let a = Args::parse(args, cmd)?;
    if a.has("report") {
        exit_with_parent();
    }
    if a.value("relay").is_some()
        || a.has("devices")
        || a.value("revoke").is_some()
        || a.value("device").is_some()
        || a.value("kick").is_some()
        || a.has("enable")
        || a.has("disable")
        || a.has("status")
    {
        return crate::relay::run(&a);
    }
    if !a.has("stdio") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("mooring remote 要选一种用法：\n{}", cmd.usage),
        ));
    }
    // `--role` narrows what this link may do, as a paired device's role would.
    let role = a.value("role").map(devices::parse_role).transpose()?;
    let access = Access {
        role: role.unwrap_or(Role::Full),
        ..Access::trusted()
    };
    serve(io::stdin().lock(), io::stdout(), &a.socket, access)
}

/// Serves `input` until it ends, replying on `output`.
pub fn serve(
    mut input: impl BufRead,
    mut output: impl Write + Send + 'static,
    socket: &Path,
    access: Access,
) -> io::Result<()> {
    let (out, lines) = mpsc::sync_channel::<Value>(OUT_QUEUE);
    let writer = thread::spawn(move || {
        for line in lines {
            if writeln!(output, "{line}")
                .and_then(|_| output.flush())
                .is_err()
            {
                break;
            }
        }
    });
    let (work, jobs) = mpsc::channel::<(u64, Value, Option<String>)>();
    let streams: Streams = Default::default();
    let worker = {
        let (out, socket, access) = (out.clone(), socket.to_owned(), access.clone());
        thread::spawn(move || {
            for (id, request, client) in jobs {
                let result = unary(&socket, &request, client.as_deref());
                let op = request["op"].as_str().unwrap_or_default();
                if devices::acts(op) {
                    access.record(op, &request, &result);
                    if result.is_ok() {
                        access.tell(Activity::Acted {
                            op: op.to_owned(),
                            terminal: request["terminal"].as_str().map(str::to_owned),
                        });
                    }
                }
                let reply = match result {
                    Ok(ok) => json!({ "id": id, "ok": ok }),
                    Err(e) => json!({ "id": id, "error": e.to_string() }),
                };
                if out.send(reply).is_err() {
                    break;
                }
            }
        })
    };
    let mut greeted = false;
    // What `hello` named this client, for the ids its writes carry.
    let mut client: Option<String> = None;
    let mut line = Vec::new();
    loop {
        line.clear();
        if input
            .by_ref()
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut line)?
            == 0
        {
            break;
        }
        if line.len() as u64 > MAX_LINE {
            let _ = out.send(json!({ "error": "一行超过 1 MiB，连接结束" }));
            break;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let request: Value = match serde_json::from_slice(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = out.send(json!({ "error": format!("不是 JSON：{e}") }));
                continue;
            }
        };
        let Some(id) = request["id"].as_u64() else {
            let _ = out.send(json!({ "error": "缺少数字 id" }));
            continue;
        };
        let op = request["op"].as_str().unwrap_or_default();
        let fail = |message: String| {
            let _ = out.send(json!({ "id": id, "error": message }));
        };
        match op {
            "hello" => {
                client = (request["client"].as_str())
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned);
                match hello(socket, access.role) {
                    Ok(ok) => {
                        greeted = true;
                        let _ = out.send(json!({ "id": id, "ok": ok }));
                    }
                    Err(e) => fail(format!("记不住本机标识：{e}")),
                }
            }
            _ if !greeted => fail("先发 hello".into()),
            _ if !access.role.allows(op) => fail(format!(
                "这台设备的权限是「{}」，不能 {op}",
                access.role.label()
            )),
            "events" | "watch" => {
                let (out, socket, streams) = (out.clone(), socket.to_owned(), streams.clone());
                let access = access.clone();
                thread::spawn(move || stream(id, &request, &socket, &out, &streams, &access));
            }
            "ping" => {
                let _ = out.send(json!({ "id": id, "ok": {} }));
            }
            // Reads logs for up to seconds: beside the requests, not ahead of an answer.
            "usage" => {
                let out = out.clone();
                thread::spawn(move || {
                    let _ = out.send(match usage::usage(&request) {
                        Ok(ok) => json!({ "id": id, "ok": ok }),
                        Err(e) => json!({ "id": id, "error": e.to_string() }),
                    });
                });
            }
            // Reads a log's lines, seconds at the worst: beside the requests too.
            "toolcalls" | "toolcall" | "backfill" => {
                let (out, socket) = (out.clone(), socket.to_owned());
                let op = op.to_owned();
                thread::spawn(move || {
                    let result = match op.as_str() {
                        "toolcalls" => tools::tool_calls(&socket, &request),
                        "toolcall" => tools::tool_call(&socket, &request),
                        _ => chat::backfill(&socket, &request),
                    };
                    let _ = out.send(match result {
                        Ok(ok) => json!({ "id": id, "ok": ok }),
                        Err(e) => json!({ "id": id, "error": e.to_string() }),
                    });
                });
            }
            "resize" => match resize(&streams, &request) {
                Ok(()) => {
                    let _ = out.send(json!({ "id": id, "ok": {} }));
                }
                Err(e) => fail(e.to_string()),
            },
            "close" => match request["stream"].as_u64() {
                Some(s) => {
                    if let Ok(mut r) = streams.lock() {
                        match r.open.remove(&s) {
                            Some(Some(sock)) => {
                                let _ = sock.shutdown(std::net::Shutdown::Both);
                            }
                            // Not open yet: it ends as soon as it would.
                            _ => {
                                r.open.insert(s, None);
                            }
                        }
                    }
                    let _ = out.send(json!({ "id": id, "ok": {} }));
                }
                None => fail("close 需要 stream".into()),
            },
            _ if OPS.contains(&op) => {
                let _ = work.send((id, request, client.clone()));
            }
            _ => fail(format!("不认识的 op {op:?}")),
        }
    }
    // The client is gone: end every stream, finish queued requests, flush the replies.
    if let Ok(mut r) = streams.lock() {
        r.ended = true;
        for sock in r.open.drain().filter_map(|(_, s)| s) {
            let _ = sock.shutdown(std::net::Shutdown::Both);
        }
    }
    drop(work);
    let _ = worker.join();
    drop(out);
    let _ = writer.join();
    Ok(())
}

pub(crate) use mooring_host::devices::machine;
fn hello(socket: &Path, role: Role) -> io::Result<Value> {
    Ok(json!(rp::Hello {
        version: rp::VERSION,
        machine: machine(socket)?,
        ops: OPS
            .iter()
            .filter(|op| role.allows(op))
            .map(|op| op.to_string())
            .collect(),
        role,
        requests: true,
        os: Some(
            if cfg!(target_os = "macos") {
                "macos"
            } else {
                "linux"
            }
            .into()
        ),
        app_version: Some(env!("CARGO_PKG_VERSION").into()),
    }))
}

/// A write its client named: it runs once however often it comes (`Request::Once`).
fn named(client: Option<&str>, request: &Value, inner: Request) -> Request {
    match named_id(client, request) {
        Some(request) => Request::Once {
            request,
            inner: Box::new(inner),
        },
        None => inner,
    }
}
/// The id a write carries, unique to its client (named in `hello`).
fn named_id(client: Option<&str>, request: &Value) -> Option<RequestId> {
    match (client, request["request"].as_str()) {
        (Some(client), Some(id)) if !id.is_empty() => Some(RequestId {
            client: format!("remote:{client}"),
            id: id.to_owned(),
        }),
        _ => None,
    }
}

fn text<'a>(request: &'a Value, key: &str) -> io::Result<&'a str> {
    request[key]
        .as_str()
        .ok_or_else(|| io::Error::other(format!("缺少 {key}")))
}
fn ok_reply(reply: Reply) -> io::Result<Value> {
    match reply {
        Reply::Ok => Ok(json!({})),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}
fn unary(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let terminal = || text(request, "terminal").map(str::to_owned);
    match request["op"].as_str().unwrap_or_default() {
        "list" => match rpc(socket, &Request::Workspace)? {
            Reply::Workspace(w) => {
                // Which terminals run an Agent no conversation is tied to; without it they
                // still list, as shells.
                let seen = match rpc(socket, &Request::RecognitionStatus) {
                    Ok(Reply::Recognition(r)) => r,
                    _ => Default::default(),
                };
                list::list(&w, &seen, socket)
            }
            other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        },
        "read" => {
            let lines = request["lines"].as_u64().unwrap_or(50).clamp(1, 2000) as usize;
            match rpc(
                socket,
                &Request::ReadScreen {
                    id: terminal()?,
                    rows: lines + 200,
                },
            )? {
                Reply::Text(t) => {
                    // The empty rows below the cursor say nothing.
                    let rows: Vec<_> = t.trim_end().lines().collect();
                    Ok(json!({ "text": rows[rows.len().saturating_sub(lines)..].join("\n") }))
                }
                other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
            }
        }
        "send" => {
            asks::not_asking_trust(socket, &terminal()?)?;
            let send = Request::SendText {
                session: match request.get("session") {
                    None => None,
                    Some(serde_json::Value::String(s)) => Some(s.clone()),
                    Some(_) => return Err(io::Error::other("session 需要字符串")),
                },
                id: terminal()?,
                text: text(request, "text")?.to_owned(),
                submit: request["enter"].as_bool().unwrap_or(false),
            };
            ok_reply(rpc(socket, &named(client, request, send))?)
        }
        "keys" => {
            let keys = request["keys"]
                .as_array()
                .ok_or_else(|| io::Error::other("keys 需要字符串数组"))?
                .iter()
                .map(|k| k.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| io::Error::other("keys 需要字符串数组"))?;
            let keys = Request::SendKeys {
                id: terminal()?,
                keys,
            };
            ok_reply(rpc(socket, &named(client, request, keys))?)
        }
        "input" => {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(text(request, "data")?)
                .map_err(|e| io::Error::other(format!("data 不是 base64：{e}")))?;
            let input = Request::SendInput {
                id: terminal()?,
                bytes,
            };
            ok_reply(rpc(socket, &named(client, request, input))?)
        }
        "answer" => {
            let (id, question) = (terminal()?, text(request, "question")?.to_owned());
            asks::same_subject(socket, &id, request)?;
            let allow = request["allow"].as_bool();
            let answer = match request["always"].as_bool().unwrap_or(false) {
                true if allow == Some(false) => {
                    Err(io::Error::other("always 只能和 allow: true 一起"))
                }
                true => Ok(Request::AnswerAlways { id, question }),
                false => allow
                    .map(|allow| Request::AnswerApproval {
                        id,
                        question,
                        allow,
                    })
                    .ok_or_else(|| io::Error::other("answer 需要 allow: true 或 false")),
            }?;
            ok_reply(rpc(socket, &named(client, request, answer))?)
        }
        "ask" => asks::ask(socket, &terminal()?),
        "trust" => asks::trust(socket, request, client),
        "chat" => chat::chat(socket, request),
        "diff" => diff::diff(socket, request),
        "dirs" => dirs::dirs(socket, request),
        "quota" => quota::quota(socket),
        "lastsession" => resume::last_session(socket, request),
        "addspace" => writes::add_space(socket, request, client),
        "mkdir" => writes::mkdir(request),
        "stop" => writes::stop(socket, request, client),
        "open" => writes::open(socket, request, client),
        "end" => writes::end(socket, request, client),
        "outcome" => writes::outcome(socket, request, client),
        op => Err(io::Error::other(format!("不认识的 op {op:?}"))),
    }
}

/// `resize`: sizes the terminal a taken stream writes to, as its pane does locally.
fn resize(streams: &Streams, request: &Value) -> io::Result<()> {
    let stream = request["stream"]
        .as_u64()
        .ok_or_else(|| io::Error::other("resize 需要 stream"))?;
    let size = |key: &str| {
        (request[key].as_u64())
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| *n > 0)
            .ok_or_else(|| io::Error::other(format!("resize 需要 {key}")))
    };
    let (cols, rows) = (size("cols")?, size("rows")?);
    let r = streams
        .lock()
        .map_err(|_| io::Error::other("streams lock poisoned"))?;
    if !r.writers.contains(&stream) {
        return Err(io::Error::other("这个画面没有接管，不能改尺寸"));
    }
    let mut socket = (r.open.get(&stream))
        .and_then(|s| s.as_ref())
        .ok_or_else(|| io::Error::other("画面已经关了"))?
        .try_clone()?;
    drop(r);
    send(
        &mut socket,
        CONTROL,
        &Request::Resize {
            cols,
            rows,
            cell_width: 0,
            cell_height: 0,
        },
    )
}
fn connect(socket: &Path, request: &Request) -> io::Result<UnixStream> {
    let mut stream = UnixStream::connect(socket)?;
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
    match receive::<Reply>(&mut stream)? {
        Reply::Hello { major: MAJOR, .. } => (),
        r => return Err(io::Error::other(format!("handshake: {r:?}"))),
    }
    send(&mut stream, CONTROL, request)?;
    Ok(stream)
}
/// One open stream: its request id, where it writes, and the registry `close` uses.
struct Stream<'a> {
    id: u64,
    out: &'a Out,
    streams: &'a Streams,
    opened: bool,
}
impl Stream<'_> {
    /// Registers the stream and tells the client it is open, before its first message.
    fn open(&mut self, socket: &UnixStream, ok: Value) -> io::Result<()> {
        let mut r = self
            .streams
            .lock()
            .map_err(|_| io::Error::other("streams lock poisoned"))?;
        if r.ended || r.open.remove(&self.id).is_some() {
            return Err(io::Error::other("closed"));
        }
        r.open.insert(self.id, Some(socket.try_clone()?));
        drop(r);
        self.opened = true;
        self.emit(json!({ "id": self.id, "ok": ok }))
    }
    fn emit(&self, message: Value) -> io::Result<()> {
        self.out
            .send(message)
            .map_err(|_| io::Error::other("client gone"))
    }
}
/// Runs one stream to its end and says why it ended; the client closing it is "closed".
fn stream(id: u64, request: &Value, socket: &Path, out: &Out, streams: &Streams, access: &Access) {
    let mut s = Stream {
        id,
        out,
        streams,
        opened: false,
    };
    let result = match request["op"].as_str() {
        Some("events") => connect(socket, &Request::Subscribe).and_then(|c| events(&mut s, c)),
        _ => text(request, "terminal").map(str::to_owned).and_then(|t| {
            access.tell(Activity::Watching(t.clone()));
            let take = request["take"].as_bool().unwrap_or(false);
            if take && !access.role.allows("resize") {
                return Err(io::Error::other("这台设备只能看，不能接管"));
            }
            let watched = watch(&mut s, &t, socket, take);
            access.tell(Activity::Unwatched(t));
            watched
        }),
    };
    // Still registered means the client did not close it.
    let open = streams
        .lock()
        .map(|mut r| r.open.remove(&id).is_some_and(|s| s.is_some()))
        .unwrap_or(false);
    let _ = match result {
        Err(e) if !s.opened => out.send(json!({ "id": id, "error": e.to_string() })),
        Err(e) if open => out.send(json!({ "stream": id, "type": "end", "reason": e.to_string() })),
        _ => out.send(json!({ "stream": id, "type": "end", "reason": "closed" })),
    };
}
fn events(s: &mut Stream, mut socket: UnixStream) -> io::Result<()> {
    let id = s.id;
    s.open(&socket, json!({ "stream": id }))?;
    loop {
        match receive::<Reply>(&mut socket)? {
            Reply::Events(notices) => {
                for n in notices {
                    s.emit(json!({ "stream": id, "type": "event", "event": n }))?;
                }
            }
            Reply::Error(e) => return Err(io::Error::other(e)),
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        }
    }
}

/// The terminal's screen as escape sequences, then its output as it comes, for an
/// emulator on the client. It only looks unless it `take`s the terminal: then it is the
/// writer, which `resize` sizes, and this machine's own panes look only.
fn watch(st: &mut Stream, terminal: &str, socket: &Path, take: bool) -> io::Result<()> {
    let (id, b64) = (st.id, base64::engine::general_purpose::STANDARD);
    let mut s = connect(
        socket,
        &Request::Attach {
            id: terminal.to_owned(),
            takeover: take,
            watch: !take,
        },
    )?;
    let (mut cols, mut rows) = match receive::<Reply>(&mut s)? {
        Reply::Attached { cols, rows, .. } => (cols, rows),
        Reply::Error(e) => return Err(io::Error::other(e)),
        other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    };
    st.open(&s, json!({ "stream": id, "cols": cols, "rows": rows }))?;
    if take {
        if let Ok(mut r) = st.streams.lock() {
            r.writers.insert(id);
        }
    }
    let mut snapshot = Vec::new();
    let mut exited = false;
    loop {
        let (kind, bytes) = read_frame(&mut s)?;
        match kind {
            SNAPSHOT => snapshot.extend_from_slice(&bytes),
            // The first screen, or the screen again after this client fell behind.
            READY => {
                let vt = Vt::restore(&std::mem::take(&mut snapshot))?;
                st.emit(json!({
                        "stream": id, "type": "screen", "cols": cols, "rows": rows,
                    "vt": b64.encode(vt.replay()?),
                }))?;
            }
            OUTPUT => {
                st.emit(json!({ "stream": id, "type": "output", "vt": b64.encode(&bytes) }))?
            }
            RESIZED => {
                (cols, rows) = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                st.emit(json!({ "stream": id, "type": "resized", "cols": cols, "rows": rows }))?;
            }
            STATUS => {
                let t: Terminal = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                if t.state != ProcessState::Running && !exited {
                    exited = true;
                    st.emit(json!({ "stream": id, "type": "exited", "code": t.exit_code }))?;
                }
            }
            _ => (),
        }
    }
}
