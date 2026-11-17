use fibo_core::outbox::{Outbox, Refused};
use fibo_core::{protocol::*, terminal::Vt};
use fibo_platform::local;
use futures::channel::mpsc::UnboundedSender;
use std::io;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
/// Input waiting for the host beyond this is refused: no hand types it, a runaway does.
const OUTBOX_LIMIT: usize = 1 << 20;

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
    outbox: Arc<Outbox>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.outbox.abandon();
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
        let outbox = Arc::new(Outbox::new(OUTBOX_LIMIT));
        let receiver = Arc::clone(&outbox);
        let unsent = Arc::clone(&outbox);
        let result = Self {
            state: Arc::clone(&state),
            dirty: Arc::clone(&dirty),
            bells: Arc::clone(&bells),
            clipboard: Arc::clone(&clipboard),
            outbox,
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
                    // Whatever queued while the last batch was on its way goes together.
                    'batches: while let Some(batch) = receiver.take() {
                        for request in batch {
                            let sent = match &request {
                                Request::Input { bytes, lossy } => {
                                    send_input(&mut writer, bytes, *lossy)
                                }
                                _ => send(&mut writer, CONTROL, &request),
                            };
                            if let Err(error) = sent {
                                receiver.abandon();
                                let mut state = write_state.lock().unwrap();
                                state.error =
                                    Some(t!("{error}；输入结果未知，请勿自动重发", error));
                                state.writable = false;
                                let _ = write_wake.unbounded_send(());
                                let _ = writer.shutdown(std::net::Shutdown::Both);
                                break 'batches;
                            }
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
                unsent.abandon();
                let mut state = state.lock().unwrap();
                state.error = Some(t!("连接已断开：{error}", error));
                state.dropped = !state.ended;
                state.writable = false;
                if let Some(socket) = &state.socket {
                    let _ = socket.shutdown(std::net::Shutdown::Both);
                }
                let _ = wake.unbounded_send(());
            }
        });
        result
    }
    pub fn send(&self, request: Request) -> Result<(), String> {
        let state = self.state.lock().map_err(|_| "terminal lock poisoned")?;
        if state.vt.is_none() || !state.writable {
            return Err(t!("终端尚未恢复或当前为只读").into());
        }
        drop(state);
        self.outbox.push(request).map_err(|refused| match refused {
            Refused::Full => t!("还有 1 MiB 输入没送到终端，这次的没发；未自动重试").into(),
            Refused::Closed => t!("连接已断开，这次的输入没发；未自动重试").into(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn input_refusals_describe_the_state_and_never_replay_after_promotion() {
        let connection = Connection {
            state: Arc::new(Mutex::new(State {
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
            })),
            dirty: Arc::new(AtomicBool::new(false)),
            bells: Arc::new(AtomicU64::new(0)),
            clipboard: Arc::new(Mutex::new(None)),
            outbox: Arc::new(Outbox::new(OUTBOX_LIMIT)),
        };
        let input = || Request::Input {
            bytes: b"blocked".to_vec(),
            lossy: false,
        };
        assert_eq!(
            connection.send(input()).unwrap_err(),
            t!("正在恢复终端，这次的输入没发")
        );
        connection.state.lock().unwrap().vt = Some(Vt::new(80, 24).unwrap());
        assert_eq!(
            connection.send(input()).unwrap_err(),
            t!("当前为只读，请先点击「接管输入」；这次的输入没发")
        );
        connection.state.lock().unwrap().dropped = true;
        assert_eq!(
            connection.send(input()).unwrap_err(),
            t!("连接已断开，正在重连；这次的输入没发")
        );
        connection.state.lock().unwrap().ended = true;
        assert_eq!(
            connection.send(input()).unwrap_err(),
            t!("进程已退出，这次的输入没发")
        );
        {
            let mut state = connection.state.lock().unwrap();
            state.ended = false;
            state.dropped = false;
            state.writable = true;
        }
        connection
            .send(Request::Input {
                bytes: b"after-promotion".to_vec(),
                lossy: false,
            })
            .unwrap();
        let sent = connection.outbox.take().unwrap();
        assert_eq!(sent.len(), 1);
        assert!(matches!(&sent[0], Request::Input { bytes, .. } if bytes == b"after-promotion"));
    }

    #[test]
    fn a_first_connection_failure_wakes_the_pane_for_retry() {
        let path = std::env::temp_dir().join(format!("fibo-absent-{}.sock", std::process::id()));
        let (tx, mut wake) = futures::channel::mpsc::unbounded();
        let connection = Connection::new(&path, "missing", false, tx);
        futures::executor::block_on(wake.next()).unwrap();
        let state = connection.state.lock().unwrap();
        assert!(state.vt.is_none());
        assert!(state.error.is_some());
        assert!(
            state.dropped,
            "the pane must retry even before its first snapshot"
        );
        assert!(!state.writable);
    }
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
