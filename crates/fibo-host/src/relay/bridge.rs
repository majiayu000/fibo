use super::connect::to_phone;
use super::squeeze::{Squeeze, Unsqueeze};
use super::INPUT_QUEUE;
use crate::remote::{self, Access};
use fibo_platform::local;
use fibo_remote::{
    close,
    secure::{self, Channel, Message as Secure},
    wire::Frame,
};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc as std_mpsc, Arc, Condvar, Mutex};
use std::thread;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// What a session's bytes reach on this machine.
pub(super) enum Serve {
    /// A phone: `remote::serve` on one end of a socket pair, with what it may do.
    Phone(Access),
    /// Another computer: this machine's own socket, the native protocol, as `ssh -L` to it.
    Native,
}

/// One device's session: the Noise channel to it on one end, `Serve` on the other.
pub(super) struct Bridge {
    channel: Arc<Mutex<Channel>>,
    /// What to write to the session, and how many bytes of the device's it was.
    input: std_mpsc::SyncSender<(Vec<u8>, u32)>,
    credit: Arc<Credit>,
    ours: local::Stream,
    /// A computer's stream comes compressed (`squeeze.rs`); a phone's does not.
    unsqueeze: Option<Mutex<Unsqueeze>>,
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
        out: mpsc::UnboundedSender<Message>,
        socket: &Path,
        proto: u32,
        serve: Serve,
    ) -> io::Result<Self> {
        let (ours, phone) = match serve {
            Serve::Phone(access) => {
                let (ours, theirs) = local::Stream::pair()?;
                (ours, Some((theirs, access)))
            }
            Serve::Native => (local::Stream::connect(socket)?, None),
        };
        let (squeeze, unsqueeze) = match phone {
            Some(_) => (None, None),
            None => (Some(Squeeze::new()?), Some(Mutex::new(Unsqueeze::new()?))),
        };
        // Allocate every descriptor before starting workers, so failure drops them all.
        let phone = match phone {
            Some((theirs, access)) => Some((theirs.try_clone()?, theirs, access)),
            None => None,
        };
        let mut writer = ours.try_clone()?;
        let mut reader = ours.try_clone()?;
        let channel = Arc::new(Mutex::new(channel));
        let credit = Arc::new(Credit::default());
        if let Some((remote_reader, theirs, access)) = phone {
            let socket: PathBuf = socket.into();
            thread::spawn(move || {
                let _ = remote::serve(BufReader::new(remote_reader), theirs, &socket, access);
            });
        }
        let (input, inbox) = std_mpsc::sync_channel::<(Vec<u8>, u32)>(INPUT_QUEUE);
        {
            let (channel, out, id) = (channel.clone(), out.clone(), id.to_owned());
            thread::spawn(move || {
                for (bytes, received) in inbox {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                    // Acknowledge bytes actually accepted by the session, before the
                    // whole JSON request has arrived (a slow image can take minutes).
                    let Ok(mut channel) = channel.lock() else {
                        break;
                    };
                    let Ok(ack) = channel.ack(received) else {
                        break;
                    };
                    // Hold the channel through enqueue: ACK and reply nonces must reach
                    // the phone in the same order they were sealed.
                    if out.send(to_phone(proto, &id, &ack)).is_err() {
                        break;
                    }
                }
                let _ = writer.shutdown(std::net::Shutdown::Write);
            });
        }
        {
            let (channel, credit, id) = (channel.clone(), credit.clone(), id.to_owned());
            let mut pump = Pump {
                channel,
                credit,
                out,
                id,
                proto,
                squeeze,
            };
            thread::spawn(move || pump.run(&mut reader));
        }
        Ok(Self {
            channel,
            input,
            credit,
            ours,
            unsqueeze,
        })
    }

    /// `bytes` sealed for the device as the session's next message, counted against its
    /// window like any reply. Sent before the device has said anything, it comes first.
    pub(super) fn say(&self, bytes: &[u8]) -> io::Result<Vec<u8>> {
        self.credit
            .state
            .lock()
            .map_err(|_| io::Error::other("credit"))?
            .0 += bytes.len() as u64;
        self.channel
            .lock()
            .map_err(|_| io::Error::other("channel lock poisoned"))?
            .data(bytes)
    }

    pub(super) fn receive(
        &self,
        message: &[u8],
        authenticated: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        let opened = self
            .channel
            .lock()
            .map_err(|_| io::Error::other("channel lock poisoned"))?
            .open(message)?;
        authenticated()?;
        match opened {
            Secure::Data(bytes) => {
                let received = bytes.len() as u32;
                let bytes = match &self.unsqueeze {
                    Some(u) => u
                        .lock()
                        .map_err(|_| io::Error::other("unsqueeze"))?
                        .unpack(&bytes)?,
                    None => bytes,
                };
                self.input
                    .try_send((bytes, received))
                    .map_err(|_| io::Error::other("phone sends faster than the session reads"))
            }
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

struct Pump {
    channel: Arc<Mutex<Channel>>,
    credit: Arc<Credit>,
    out: mpsc::UnboundedSender<Message>,
    id: String,
    proto: u32,
    squeeze: Option<Squeeze>,
}

impl Pump {
    /// Sends the session's replies to the phone, at most [`secure::WINDOW`] unacknowledged;
    /// past that it stops reading, so the session's own streams slow down and a stalled
    /// watch is resynced by the host, while other phones keep flowing.
    fn run(&mut self, reader: &mut local::Stream) {
        let (credit, out, id, proto) = (&*self.credit, &self.out, &self.id[..], self.proto);
        let mut buf = vec![0; secure::CHUNK];
        'read: loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let packed = match self.squeeze.as_mut().map(|s| s.pack(&buf[..n])) {
                Some(Ok(packed)) => packed,
                Some(Err(_)) => break 'read,
                None => buf[..n].to_vec(),
            };
            // Compressed, a piece that does not compress comes out a little longer than
            // it went in: each message stays within `secure::CHUNK`.
            for piece in packed.chunks(secure::CHUNK) {
                let n = piece.len();
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
                let Ok(mut channel) = self.channel.lock() else {
                    break 'read;
                };
                let Ok(sealed) = channel.data(piece) else {
                    break 'read;
                };
                if out.send(to_phone(proto, id, &sealed)).is_err() {
                    return;
                }
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
        let _ = out.send(Message::text(frame.to_text()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_incomplete_upload_gets_acknowledged_before_any_reply() {
        use fibo_remote::secure::{initiate, respond, Code};
        let code = Code::generate().unwrap();
        let (hs, first) = initiate(&code).unwrap();
        let (machine, answer) = respond(&code, &first).unwrap();
        let mut phone = hs.finish(&answer).unwrap();
        let (out, mut received) = mpsc::unbounded_channel();
        let bridge = Bridge::start(
            "phone",
            machine,
            out,
            Path::new("/unused.sock"),
            2,
            Serve::Phone(Access {
                role: fibo_remote::role::Role::Full,
                device: None,
                log: None,
                observer: None,
            }),
        )
        .unwrap();
        let bytes = b"{\"id\":1,\"op\":\"image\",\"data\":\"partial";
        bridge
            .receive(&phone.data(bytes).unwrap(), || Ok(()))
            .unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let frame = loop {
            if let Ok(frame) = received.try_recv() {
                break frame;
            }
            assert!(
                std::time::Instant::now() < until,
                "partial uploads need progress before a full request exists"
            );
            thread::sleep(std::time::Duration::from_millis(10));
        };
        let tokio_tungstenite::tungstenite::Message::Binary(raw) = frame else {
            panic!("expected encrypted ACK")
        };
        let (_, id, sealed) = fibo_remote::wire::decode(&raw).unwrap();
        assert_eq!(id, "phone");
        assert_eq!(phone.open(sealed).unwrap(), Secure::Ack(bytes.len() as u32));
    }
}
