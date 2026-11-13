use fibo_core::{protocol::*, terminal::Vt};
use fibo_platform::local;
use futures::channel::mpsc::UnboundedSender;
use std::io;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{SyncSender, sync_channel},
};

pub struct State {
    pub vt: Option<Vt>,
    pub writable: bool,
    pub error: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub socket: Option<local::Stream>,
    pub ended: bool,
    /// A running terminal's stream broke after its screen arrived: the host drops a
    /// viewer that falls behind a burst of output. Attaching again restores it.
    pub dropped: bool,
    /// The scheme `vt` was last repainted with; a fresh screen from the host has none.
    pub scheme: Option<fibo_core::terminal::TermColors>,
    /// The cursor shape `vt` was last given; a fresh screen from the host has none.
    pub cursor_shape: Option<crate::ui::terminal_text::CursorDefault>,
}
pub struct Connection {
    pub state: Arc<Mutex<State>>,
    pub dirty: Arc<AtomicBool>,
    /// BELs the program rang since this pane connected; counted by the display copy.
    pub bells: Arc<AtomicU64>,
    /// Latest OSC 52 clipboard write from the program; the UI thread applies it.
    /// Separate from `state`: the callback runs while the reader holds that lock.
    pub clipboard: Arc<Mutex<Option<String>>>,
    sender: SyncSender<Request>,
    /// Another machine's terminal: the `watch` stream to close when this goes.
    remote: Option<(Arc<fibo_remote::client::Client>, u64)>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        if let Some((client, stream)) = self.remote.take() {
            client.close(stream);
        }
        if let Ok(s) = self.state.lock()
            && let Some(socket) = &s.socket
        {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    }
}
impl Connection {
    pub fn new(socket: &Path, id: &str, takeover: bool, wake: UnboundedSender<()>) -> Self {
        let state = Arc::new(Mutex::new(State {
            vt: None,
            writable: false,
            error: None,
            cols: 80,
            rows: 24,
            socket: None,
            ended: false,
            dropped: false,
            scheme: None,
            cursor_shape: None,
        }));
        let dirty = Arc::new(AtomicBool::new(false));
        let bells = Arc::new(AtomicU64::new(0));
        let clipboard = Arc::new(Mutex::new(None));
        let (sender, receiver) = sync_channel(128);
        let result = Self {
            state: Arc::clone(&state),
            dirty: Arc::clone(&dirty),
            bells: Arc::clone(&bells),
            clipboard: Arc::clone(&clipboard),
            sender,
            remote: None,
        };
        let socket = socket.to_owned();
        let id = id.to_owned();
        std::thread::spawn(move || {
            let work = (|| -> io::Result<()> {
                let mut stream = local::Stream::connect(socket)?;
                stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
                state.lock().unwrap().socket = Some(stream.try_clone()?);
                send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
                match receive::<Reply>(&mut stream)? {
                    Reply::Hello { major: MAJOR, .. } => (),
                    r => return Err(io::Error::other(format!("handshake: {r:?}"))),
                }
                send(
                    &mut stream,
                    CONTROL,
                    &Request::Attach {
                        id,
                        takeover,
                        watch: false,
                    },
                )?;
                match receive::<Reply>(&mut stream)? {
                    Reply::Attached {
                        writable,
                        cols,
                        rows,
                    } => {
                        let mut state = state.lock().unwrap();
                        state.writable = writable;
                        state.cols = cols;
                        state.rows = rows;
                    }
                    r => return Err(io::Error::other(format!("attach: {r:?}"))),
                }
                let mut writer = stream.try_clone()?;
                let write_state = Arc::clone(&state);
                let write_wake = wake.clone();
                std::thread::spawn(move || {
                    for request in receiver {
                        let sent = match &request {
                            Request::Input { bytes, lossy } => {
                                send_input(&mut writer, bytes, *lossy)
                            }
                            _ => send(&mut writer, CONTROL, &request),
                        };
                        if let Err(error) = sent {
                            let mut state = write_state.lock().unwrap();
                            state.error = Some(t!("{error}；输入结果未知，请勿自动重发", error));
                            state.writable = false;
                            let _ = write_wake.unbounded_send(());
                            let _ = writer.shutdown(std::net::Shutdown::Both);
                            break;
                        }
                    }
                });
                let mut snapshot = Vec::new();
                loop {
                    let (kind, bytes) = read_frame(&mut stream)?;
                    let mut state = state.lock().unwrap();
                    match kind {
                        SNAPSHOT => {
                            if snapshot.len() + bytes.len() > 64 * 1024 * 1024 {
                                return Err(io::Error::other("snapshot exceeds memory limit"));
                            }
                            snapshot.extend(bytes);
                            continue;
                        }
                        READY => {
                            let mut vt = Vt::restore(&snapshot)?;
                            let rung = Arc::clone(&bells);
                            vt.on_bell(move || {
                                rung.fetch_add(1, Ordering::Relaxed);
                            })?;
                            let slot = Arc::clone(&clipboard);
                            vt.on_clipboard_text(move |text| {
                                if let Ok(mut slot) = slot.lock() {
                                    *slot = Some(text);
                                }
                            })?;
                            state.vt = Some(vt);
                            state.scheme = None;
                            state.cursor_shape = None;
                            snapshot.clear();
                            snapshot.shrink_to_fit();
                        }
                        OUTPUT => {
                            state
                                .vt
                                .as_mut()
                                .ok_or_else(|| io::Error::other("output before snapshot"))?
                                .feed(&bytes);
                        }
                        RESIZED => {
                            let (cols, rows) =
                                serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                            state
                                .vt
                                .as_mut()
                                .ok_or_else(|| io::Error::other("resize before snapshot"))?
                                .resize(cols, rows)?;
                            state.cols = cols;
                            state.rows = rows;
                        }
                        STATUS => {
                            let session: fibo_core::model::Terminal =
                                serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                            if session.state != fibo_core::model::ProcessState::Running {
                                state.writable = false;
                                state.ended = true;
                            }
                        }
                        WRITABLE => {
                            state.writable = serde_json::from_slice::<bool>(&bytes)
                                .map_err(io::Error::other)?
                                && !state.ended;
                        }
                        ACK => {
                            if let Reply::Error(error) =
                                serde_json::from_slice::<Reply>(&bytes).map_err(io::Error::other)?
                            {
                                state.error = Some(error);
                                state.writable = false;
                            }
                        }
                        _ => return Err(io::Error::other("unknown terminal frame")),
                    }
                    drop(state);
                    if !dirty.swap(true, Ordering::AcqRel) {
                        let _ = wake.unbounded_send(());
                    }
                }
            })();
            if let Err(error) = work {
                let mut state = state.lock().unwrap();
                state.error = Some(t!("连接已断开：{error}", error));
                state.dropped = state.vt.is_some() && !state.ended;
                state.writable = false;
                if let Some(socket) = &state.socket {
                    let _ = socket.shutdown(std::net::Shutdown::Both);
                }
                let _ = wake.unbounded_send(());
            }
        });
        result
    }
    /// Another machine's terminal through `watch`, never resized from here; typed into
    /// (`input`) when that machine lets this desktop. `linked` is `None` while that
    /// machine is unreachable; the pane says so.
    /// `take`: as the writer that sizes it (`watch` with `take`); that machine's own panes
    /// then look only.
    pub fn remote(
        linked: Option<crate::machines::Linked>,
        id: &str,
        take: bool,
        wake: UnboundedSender<()>,
    ) -> Self {
        use base64::Engine;
        use fibo_remote::protocol::{Op, StreamBody};
        let state = Arc::new(Mutex::new(State {
            vt: None,
            writable: false,
            error: None,
            cols: 80,
            rows: 24,
            socket: None,
            ended: false,
            dropped: false,
            scheme: None,
            cursor_shape: None,
        }));
        let dirty = Arc::new(AtomicBool::new(false));
        let (sender, typed) = sync_channel(128);
        let mut result = Self {
            state: Arc::clone(&state),
            dirty: Arc::clone(&dirty),
            bells: Arc::new(AtomicU64::new(0)),
            clipboard: Arc::new(Mutex::new(None)),
            sender,
            remote: None,
        };
        let Some(linked) = linked else {
            state.lock().unwrap().error = Some(t!("那台电脑没连上").into());
            return result;
        };
        let client = linked.client;
        let take = take && linked.resizing;
        state.lock().unwrap().writable = linked.typing;
        let on = {
            let (state, dirty, wake) = (Arc::clone(&state), Arc::clone(&dirty), wake.clone());
            move |body: StreamBody| {
                let b64 = base64::engine::general_purpose::STANDARD;
                let mut s = state.lock().unwrap();
                let fed = match body {
                    StreamBody::Screen { cols, rows, vt } => {
                        b64.decode(vt).map_err(io::Error::other).and_then(|bytes| {
                            let mut fresh = Vt::new(cols, rows)?;
                            fresh.feed(&bytes);
                            s.vt = Some(fresh);
                            s.scheme = None;
                            s.cursor_shape = None;
                            s.cols = cols;
                            s.rows = rows;
                            Ok(())
                        })
                    }
                    StreamBody::Output { vt } => {
                        b64.decode(vt).map_err(io::Error::other).map(|bytes| {
                            if let Some(v) = s.vt.as_mut() {
                                v.feed(&bytes);
                            }
                        })
                    }
                    StreamBody::Resized { cols, rows } => match s.vt.as_mut() {
                        Some(v) => v.resize(cols, rows).map(|()| {
                            s.cols = cols;
                            s.rows = rows;
                        }),
                        None => Ok(()),
                    },
                    StreamBody::Exited { .. } => {
                        s.ended = true;
                        Ok(())
                    }
                    StreamBody::End { reason } => {
                        if !s.ended {
                            s.error = Some(t!("连接已断开：{error}", error = reason));
                            s.dropped = s.vt.is_some();
                        }
                        Ok(())
                    }
                    StreamBody::Event { .. } => Ok(()),
                };
                if let Err(e) = fed {
                    s.error = Some(t!("画面读不懂：{e}", e));
                }
                drop(s);
                if !dirty.swap(true, Ordering::AcqRel) {
                    let _ = wake.unbounded_send(());
                }
            }
        };
        let opened = client.stream(
            Op::Watch {
                terminal: id.to_owned(),
                take,
            },
            on,
            std::time::Duration::from_secs(10),
        );
        match opened {
            Ok((stream, _)) => {
                if linked.typing {
                    let (client, id) = (Arc::clone(&client), id.to_owned());
                    let (state, wake) = (Arc::clone(&state), wake.clone());
                    std::thread::spawn(move || {
                        let sizes = take.then_some(stream);
                        if let Err(e) = remote_typing(&client, &id, sizes, &typed) {
                            state.lock().unwrap().error = Some(t!("没发过去：{e}", e));
                            let _ = wake.unbounded_send(());
                        }
                    });
                }
                result.remote = Some((client, stream));
            }
            Err(e) => {
                state.lock().unwrap().error = Some(t!("打不开那台电脑的终端：{e}", e));
                let _ = wake.unbounded_send(());
            }
        }
        result
    }
    pub fn send(&self, request: Request) -> Result<(), String> {
        let state = self.state.lock().map_err(|_| "terminal lock poisoned")?;
        // Another machine's terminal queues what is typed while its screen is on the way.
        if (state.vt.is_none() && self.remote.is_none()) || !state.writable {
            return Err(t!("终端尚未恢复或当前为只读").into());
        }
        drop(state);
        self.sender
            .try_send(request)
            .map_err(|_| t!("输入队列已满或连接已断开；未自动重试").into())
    }
}

/// What the status subscription reports: the host's status, or that the link to the host
/// dropped. A host can itself say recognition is Interrupted (a log folder it could not
/// read); only `Lost` means the window cannot reach the host.
#[derive(Debug, Clone, PartialEq)]
pub enum StatusLink {
    Status(Box<RecognitionStatus>),
    Lost(String),
}
impl StatusLink {
    /// For a reader that only wants a phase: a lost link reads as Interrupted.
    pub fn into_status(self) -> RecognitionStatus {
        match self {
            Self::Status(status) => *status,
            Self::Lost(detail) => RecognitionStatus {
                phase: RecognitionPhase::Interrupted,
                detail,
                ..RecognitionStatus::default()
            },
        }
    }
}

/// One blocking status subscription, independent of terminal output. No refresh timer.
pub struct RecognitionConnection {
    socket: Arc<Mutex<Option<local::Stream>>>,
    closed: Arc<AtomicBool>,
}
impl RecognitionConnection {
    pub fn new(path: &Path, wake: UnboundedSender<StatusLink>) -> Self {
        let socket = Arc::new(Mutex::new(None));
        let closed = Arc::new(AtomicBool::new(false));
        let result = Self {
            socket: Arc::clone(&socket),
            closed: Arc::clone(&closed),
        };
        let path = path.to_owned();
        std::thread::spawn(move || {
            // The host can be replaced (a newer build takes over) or restart; keep
            // subscribing so one dropped connection does not freeze the status for good.
            let mut delay = std::time::Duration::from_millis(250);
            let mut reported = false;
            loop {
                let mut subscribed = false;
                let work = (|| -> io::Result<()> {
                    let mut stream = local::Stream::connect(&path)?;
                    {
                        let mut slot = socket.lock().unwrap();
                        if closed.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        *slot = Some(stream.try_clone()?);
                    }
                    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
                    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
                    if !matches!(
                        receive::<Reply>(&mut stream)?,
                        Reply::Hello { major: MAJOR, .. }
                    ) {
                        return Err(io::Error::other(t!("识别状态协议不匹配")));
                    }
                    send(&mut stream, CONTROL, &Request::WatchRecognition)?;
                    loop {
                        match receive::<Reply>(&mut stream)? {
                            Reply::Recognition(status) => {
                                subscribed = true;
                                if wake
                                    .unbounded_send(StatusLink::Status(Box::new(status)))
                                    .is_err()
                                {
                                    return Ok(());
                                }
                            }
                            _ => return Err(io::Error::other(t!("识别状态响应无效"))),
                        }
                    }
                })();
                if closed.load(Ordering::Acquire) || wake.is_closed() {
                    return;
                }
                let error = match work {
                    Ok(()) => return,
                    Err(error) => error,
                };
                if subscribed {
                    reported = false;
                    delay = std::time::Duration::from_millis(250);
                }
                if !reported {
                    reported = true;
                    let lost = StatusLink::Lost(t!("识别状态连接中断：{error}", error));
                    if wake.unbounded_send(lost).is_err() {
                        return;
                    }
                }
                std::thread::sleep(delay);
                delay = (delay * 2).min(std::time::Duration::from_secs(5));
            }
        });
        result
    }
}
impl Drop for RecognitionConnection {
    fn drop(&mut self) {
        let socket = self.socket.lock().unwrap();
        self.closed.store(true, Ordering::Release);
        if let Some(socket) = &*socket {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Sends what is typed into another machine's terminal, in order: whatever queued while
/// the last one was on its way goes as one `input`, so a fast typist waits one round
/// trip, not one per key. Each carries a request id, so that machine types it once.
/// With `sizes` (the taken stream), the pane's size changes go too, as `resize`.
fn remote_typing(
    client: &fibo_remote::client::Client,
    terminal: &str,
    sizes: Option<u64>,
    typed: &std::sync::mpsc::Receiver<Request>,
) -> io::Result<()> {
    use base64::Engine;
    use fibo_remote::protocol::Op;
    let wait = std::time::Duration::from_secs(10);
    let mut next = typed.recv().ok();
    while let Some(request) = next.take() {
        let mut bytes = match request {
            Request::Input { bytes, .. } => bytes,
            Request::Resize { cols, rows, .. } => {
                if let Some(stream) = sizes {
                    client.request(Op::Resize { stream, cols, rows }, wait)?;
                }
                next = typed.recv().ok();
                continue;
            }
            _ => Vec::new(),
        };
        // Input queued meanwhile joins this one; anything else waits its turn.
        while bytes.len() < 32 << 10 {
            match typed.try_recv() {
                Ok(Request::Input { bytes: more, .. }) => bytes.extend(more),
                Ok(other) => {
                    next = Some(other);
                    break;
                }
                Err(_) => break,
            }
        }
        if !bytes.is_empty() {
            let op = Op::Input {
                terminal: terminal.to_owned(),
                data: base64::engine::general_purpose::STANDARD.encode(&bytes),
                request: Some(fibo_core::protocol::RequestId::fresh().id),
            };
            client.request(op, wait)?;
        }
        if next.is_none() {
            next = typed.recv().ok();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    /// Plays one host lifetime: greets a single subscriber, reports `phase`, then exits.
    fn host_once(path: &Path, phase: RecognitionPhase) -> std::thread::JoinHandle<()> {
        let _ = std::fs::remove_file(path);
        let listener = local::Listener::bind(path).unwrap();
        std::thread::spawn(move || {
            let mut stream = listener.accept().unwrap();
            let _: Request = receive(&mut stream).unwrap();
            let hello = Reply::Hello {
                major: MAJOR,
                instance: "test".into(),
                build: Build::default(),
            };
            send(&mut stream, CONTROL, &hello).unwrap();
            let _: Request = receive(&mut stream).unwrap();
            let status = RecognitionStatus {
                phase,
                ..RecognitionStatus::default()
            };
            send(&mut stream, CONTROL, &Reply::Recognition(status)).unwrap();
        })
    }

    #[test]
    fn recognition_resubscribes_after_the_host_is_replaced() {
        let dir = std::env::temp_dir().join(format!("kl-recog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("host.sock");
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let first = host_once(&path, RecognitionPhase::Starting);
        let _connection = RecognitionConnection::new(&path, tx);
        let mut next = || futures::executor::block_on(rx.next()).unwrap();
        let phase = |link: StatusLink| match link {
            StatusLink::Status(status) => status.phase,
            StatusLink::Lost(detail) => panic!("lost: {detail}"),
        };
        assert_eq!(phase(next()), RecognitionPhase::Starting);
        first.join().unwrap();
        assert!(matches!(next(), StatusLink::Lost(_)));
        host_once(&path, RecognitionPhase::Healthy).join().unwrap();
        assert_eq!(phase(next()), RecognitionPhase::Healthy);
        let _ = std::fs::remove_dir_all(dir);
    }
}
