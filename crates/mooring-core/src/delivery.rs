//! One request's life on a client (the window, the `mooring` command): sent, then
//! confirmed, refused, or of unknown outcome. A named edit (`RequestId`) applies once
//! however often it is sent, and the reads change nothing, so a dropped link or a host
//! restarting is waited out by sending the same request again; only after the patience
//! runs out does the sender hear that the outcome is unknown, never nothing.
use crate::protocol::{CONTROL, MAJOR, receive, send};
use crate::protocol::{Reply, Request};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long the window and the CLI keep resending before saying the outcome is unknown:
/// longer than a host takes to restart and hand its terminals over.
pub const PATIENCE: Duration = Duration::from_secs(10);

// One per request and gone once read: the size of a workspace reply costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Settled {
    Confirmed(Reply),
    /// The host decided against it, for a reason that a resend gets again.
    Refused(String),
    /// No reply. `sent`: the request may have reached the host.
    Unknown {
        sent: bool,
        error: io::Error,
    },
}

impl Settled {
    /// What to tell the user when it did not go through.
    pub fn words(&self) -> Option<String> {
        match self {
            Self::Confirmed(_) => None,
            Self::Refused(e) => Some(e.clone()),
            Self::Unknown { sent: false, error } => {
                Some(t!("没发出去：后台没有响应（{error}）", error))
            }
            Self::Unknown { sent: true, error } => {
                Some(t!("不确定是否成功：等回复时断开了（{error}）", error))
            }
        }
    }
    pub fn into_result(self) -> io::Result<Reply> {
        match self {
            Self::Confirmed(reply) => Ok(reply),
            Self::Refused(e) => Err(io::Error::other(e)),
            Self::Unknown { error, .. } => Err(error),
        }
    }
}

/// Safe to send again after it may have arrived: a named edit, a launch (its id makes
/// it once), or one that only reads.
pub fn resendable(request: &Request) -> bool {
    match request {
        Request::Edit { request, .. } => request.is_some(),
        Request::Launch { .. }
        | Request::Once { .. }
        | Request::Workspace
        | Request::RecognitionStatus
        | Request::ReloadPlugins => true,
        _ => false,
    }
}

/// One try. The error keeps its kind and says at which step it failed.
pub fn attempt(socket: &Path, request: &Request) -> Settled {
    let stage = |name, e: io::Error| io::Error::new(e.kind(), format!("RPC {name}: {e}"));
    let unsent = |name, e| Settled::Unknown {
        sent: false,
        error: stage(name, e),
    };
    let mut stream = match mooring_platform::local::Stream::connect(socket) {
        Ok(s) => s,
        Err(e) => return unsent("connect", e),
    };
    let timeouts = stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .and_then(|()| stream.set_write_timeout(Some(Duration::from_secs(10))));
    if let Err(e) = timeouts {
        return unsent("connect", e);
    }
    if let Err(e) = send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }) {
        return unsent("send hello", e);
    }
    match receive::<Reply>(&mut stream) {
        Ok(Reply::Hello { major: MAJOR, .. }) => (),
        Ok(reply) => return Settled::Refused(format!("handshake: {reply:?}")),
        Err(e) => return unsent("receive hello", e),
    }
    if let Err(e) = send(&mut stream, CONTROL, request) {
        return Settled::Unknown {
            sent: true,
            error: stage("send request", e),
        };
    }
    match receive::<Reply>(&mut stream) {
        Ok(Reply::Error(message)) => Settled::Refused(message),
        Ok(reply) => Settled::Confirmed(reply),
        Err(e) => Settled::Unknown {
            sent: true,
            error: stage("receive reply", e),
        },
    }
}

/// Tries until it is settled, resending while that is safe and `patience` lasts.
pub fn deliver(socket: &Path, request: &Request, patience: Duration) -> Settled {
    let start = Instant::now();
    let mut wait = Duration::from_millis(100);
    let mut ever_sent = false;
    loop {
        let mut settled = attempt(socket, request);
        if let Settled::Unknown { sent, .. } = &mut settled {
            ever_sent |= *sent;
            *sent = ever_sent;
        }
        let again = match &settled {
            Settled::Unknown { sent, .. } => !sent || resendable(request),
            _ => false,
        };
        if !again || start.elapsed() + wait > patience {
            return settled;
        }
        std::thread::sleep(wait);
        wait = (wait * 2).min(Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Edit;
    use crate::protocol::{RequestId, read_frame};
    use mooring_platform::local;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A host that answers the handshake, reads the request, then does `then`.
    fn host(
        then: fn(&mut mooring_platform::local::Stream),
    ) -> (std::path::PathBuf, Arc<AtomicUsize>) {
        let dir = std::env::temp_dir().join(format!("kl-dl-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("s");
        let listener = local::Listener::bind(&socket).unwrap();
        let tries = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&tries);
        std::thread::spawn(move || {
            while let Ok(mut stream) = listener.accept() {
                counted.fetch_add(1, Ordering::SeqCst);
                let _ = read_frame(&mut stream);
                let hello = Reply::Hello {
                    major: MAJOR,
                    instance: String::new(),
                    build: Default::default(),
                };
                let _ = send(&mut stream, CONTROL, &hello);
                let _ = read_frame(&mut stream);
                then(&mut stream);
            }
        });
        (socket, tries)
    }
    fn named() -> Request {
        Request::Edit {
            request: Some(RequestId::fresh()),
            edit: Edit::CloseTab {
                id: "t".into(),
                expect: None,
            },
        }
    }
    const SHORT: Duration = Duration::from_millis(700);

    #[test]
    fn no_host_is_waited_out_then_reported_as_not_sent() {
        let socket = std::env::temp_dir().join("kl-dl-missing/s");
        let start = Instant::now();
        let settled = deliver(&socket, &named(), SHORT);
        assert!(start.elapsed() >= Duration::from_millis(300));
        assert!(matches!(settled, Settled::Unknown { sent: false, .. }));
        assert!(settled.words().unwrap().starts_with("没发出去"));
    }

    #[test]
    fn a_hang_up_after_sending_is_resent_only_when_that_is_safe() {
        let (socket, tries) = host(|_| ());
        let settled = deliver(&socket, &named(), SHORT);
        assert!(matches!(settled, Settled::Unknown { sent: true, .. }));
        assert!(settled.words().unwrap().starts_with("不确定是否成功"));
        assert!(tries.load(Ordering::SeqCst) > 1);
        // Unnamed, it may already have happened: sent once, then reported.
        let (socket, tries) = host(|_| ());
        let stop = Request::Stop { id: "t".into() };
        assert!(matches!(
            deliver(&socket, &stop, SHORT),
            Settled::Unknown { sent: true, .. }
        ));
        assert_eq!(tries.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_received_request_stays_unknown_after_the_host_disappears() {
        let root = std::env::temp_dir().join(format!("kl-dl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let socket = root.join("s");
        let listener = local::Listener::bind(&socket).unwrap();
        let gone = socket.clone();
        let server = std::thread::spawn(move || {
            let mut stream = listener.accept().unwrap();
            receive::<Request>(&mut stream).unwrap();
            send(
                &mut stream,
                CONTROL,
                &Reply::Hello {
                    major: MAJOR,
                    instance: String::new(),
                    build: Default::default(),
                },
            )
            .unwrap();
            assert!(matches!(
                receive::<Request>(&mut stream).unwrap(),
                Request::Edit { .. }
            ));
            drop(listener);
            std::fs::remove_file(gone).unwrap();
            // The request arrived, but its receipt and the next connection are lost.
        });
        let settled = deliver(&socket, &named(), SHORT);
        server.join().unwrap();
        assert!(
            matches!(settled, Settled::Unknown { sent: true, .. }),
            "{settled:?}"
        );
        assert!(settled.words().unwrap().starts_with("不确定是否成功"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_refusal_and_a_confirmation_end_it_at_once() {
        let (socket, tries) = host(|s| {
            let _ = send(s, CONTROL, &Reply::Error("标签不存在".into()));
        });
        let settled = deliver(&socket, &named(), SHORT);
        assert_eq!(settled.words().as_deref(), Some("标签不存在"));
        assert_eq!(tries.load(Ordering::SeqCst), 1);
        let (socket, tries) = host(|s| {
            let _ = send(s, CONTROL, &Reply::Ok);
        });
        assert!(matches!(
            deliver(&socket, &named(), SHORT),
            Settled::Confirmed(Reply::Ok)
        ));
        assert_eq!(tries.load(Ordering::SeqCst), 1);
    }
}
