use super::connect::to_phone;
use super::INPUT_QUEUE;
use crate::remote::{self, Access};
use mooring_remote::{
    close,
    secure::{self, Channel, Message as Secure},
    wire::Frame,
};
use std::io::{self, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{mpsc as std_mpsc, Arc, Condvar, Mutex};
use std::thread;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// One phone's session: `remote::serve` on one end of a socket pair, the Noise channel
/// to the phone on the other.
pub(super) struct Bridge {
    channel: Arc<Mutex<Channel>>,
    input: std_mpsc::SyncSender<Vec<u8>>,
    credit: Arc<Credit>,
    ours: UnixStream,
}

/// DATA bytes in flight to the phone, unacknowledged.
#[derive(Default)]
struct Credit {
    state: Mutex<(u64, bool)>,
    changed: Condvar,
}

impl Bridge {
    pub(super) fn start(
        id: &str,
        channel: Channel,
        out: mpsc::Sender<Message>,
        socket: &Path,
        proto: u32,
        access: Access,
    ) -> io::Result<Self> {
        let (ours, theirs) = UnixStream::pair()?;
        // Allocate every descriptor before starting workers, so failure drops them all.
        let remote_reader = theirs.try_clone()?;
        let mut writer = ours.try_clone()?;
        let mut reader = ours.try_clone()?;
        let channel = Arc::new(Mutex::new(channel));
        let credit = Arc::new(Credit::default());
        {
            let socket: PathBuf = socket.into();
            thread::spawn(move || {
                let _ = remote::serve(BufReader::new(remote_reader), theirs, &socket, access);
            });
        }
        let (input, inbox) = std_mpsc::sync_channel::<Vec<u8>>(INPUT_QUEUE);
        {
            thread::spawn(move || {
                for bytes in inbox {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                }
                let _ = writer.shutdown(std::net::Shutdown::Write);
            });
        }
        {
            let (channel, credit, id) = (channel.clone(), credit.clone(), id.to_owned());
            thread::spawn(move || pump(&mut reader, &channel, &credit, &out, &id, proto));
        }
        Ok(Self {
            channel,
            input,
            credit,
            ours,
        })
    }

    pub(super) fn receive(&self, message: &[u8]) -> io::Result<()> {
        let opened = self
            .channel
            .lock()
            .map_err(|_| io::Error::other("channel lock poisoned"))?
            .open(message)?;
        match opened {
            Secure::Data(bytes) => self
                .input
                .try_send(bytes)
                .map_err(|_| io::Error::other("phone sends faster than the session reads")),
            Secure::Ack(n) => {
                let mut s = self
                    .credit
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("credit"))?;
                s.0 = s.0.saturating_sub(n.into());
                self.credit.changed.notify_all();
                Ok(())
            }
        }
    }
}

impl Drop for Bridge {
    /// Ends the session; the pump sees it was ended here and says nothing more.
    fn drop(&mut self) {
        if let Ok(mut s) = self.credit.state.lock() {
            s.1 = true;
        }
        self.credit.changed.notify_all();
        let _ = self.ours.shutdown(std::net::Shutdown::Both);
    }
}

/// Sends the session's replies to the phone, at most [`secure::WINDOW`] unacknowledged;
/// past that it stops reading, so the session's own streams slow down and a stalled
/// watch is resynced by the host, while other phones keep flowing.
fn pump(
    reader: &mut UnixStream,
    channel: &Mutex<Channel>,
    credit: &Credit,
    out: &mpsc::Sender<Message>,
    id: &str,
    proto: u32,
) {
    let mut buf = vec![0; secure::CHUNK];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        {
            let Ok(mut s) = credit.state.lock() else {
                return;
            };
            while !s.1 && s.0 + n as u64 > secure::WINDOW {
                s = match credit.changed.wait(s) {
                    Ok(s) => s,
                    Err(_) => return,
                };
            }
            if s.1 {
                return;
            }
            s.0 += n as u64;
        }
        let sealed = match channel.lock().map(|mut c| c.data(&buf[..n])) {
            Ok(Ok(sealed)) => sealed,
            _ => break,
        };
        if out.blocking_send(to_phone(proto, id, &sealed)).is_err() {
            return;
        }
    }
    // The session ended on its own, not by a dropped bridge: let the phone know.
    if credit.state.lock().map_or(true, |s| s.1) {
        return;
    }
    let frame = Frame::Wclose {
        id: id.into(),
        code: Some(close::NORMAL),
        reason: None,
    };
    let _ = out.blocking_send(Message::text(frame.to_text()));
}
