//! Typing into a terminal without holding it.
//!
//! A terminal's input queue holds about 1 KiB, and a program busy printing (Codex redrawing
//! its spinner) reads more only once its own output has been taken. Taking that output needs
//! the terminal's lock, so typing under that lock left both waiting until the write gave up
//! after five seconds, with a paste cut in the middle. Here one thread per terminal types, in
//! the order asked, and whoever asked waits with no lock held.
use std::io;
use std::sync::mpsc;

use fibo_platform::terminal as term;

type Done = mpsc::SyncSender<io::Result<()>>;

struct Write {
    bytes: Vec<u8>,
    pointer: bool,
    done: Option<Done>,
}

#[derive(Clone)]
pub(super) struct Input(mpsc::Sender<Write>);

impl Input {
    /// Its thread ends once every copy is dropped and what was queued has been typed.
    pub(super) fn start(master: &term::Master, id: &str) -> io::Result<Self> {
        let mut writer = master.writer()?;
        let (tx, rx) = mpsc::channel::<Write>();
        let id = id.to_owned();
        std::thread::Builder::new()
            .name(format!("input {id}"))
            .spawn(move || {
                for Write {
                    bytes,
                    pointer,
                    done,
                } in rx
                {
                    let result = if pointer {
                        write_pointer(&mut writer, &bytes)
                    } else {
                        writer.write(&bytes)
                    };
                    match done {
                        Some(done) => {
                            let _ = done.send(result);
                        }
                        None => {
                            if let Err(e) = result {
                                eprintln!("terminal {id}: reply: {e}");
                            }
                        }
                    }
                }
            })?;
        Ok(Self(tx))
    }

    /// Types `bytes` and waits until all were taken or the write gave up. Hold no terminal
    /// or host lock while calling this: the wait can be as long as the program takes to read.
    pub(super) fn write(&self, bytes: impl Into<Vec<u8>>) -> io::Result<()> {
        self.send(bytes.into(), false)
    }

    pub(super) fn write_pointer(&self, bytes: Vec<u8>) -> io::Result<()> {
        self.send(bytes, true)
    }

    fn send(&self, bytes: Vec<u8>, pointer: bool) -> io::Result<()> {
        let (done, wait) = mpsc::sync_channel(1);
        self.0
            .send(Write {
                bytes,
                pointer,
                done: Some(done),
            })
            .map_err(|_| closed())?;
        wait.recv().map_err(|_| closed())?
    }

    /// Answers the program's own query, after any typing already queued, without waiting:
    /// the read pump calls this and must go back to reading.
    pub(super) fn reply(&self, bytes: Vec<u8>) {
        let _ = self.0.send(Write {
            bytes,
            pointer: false,
            done: None,
        });
    }
}

fn write_pointer(writer: &mut term::Writer, bytes: &[u8]) -> io::Result<()> {
    // Check each piece on the writer thread. A nearly full input queue can cut a report
    // after its ESC, leaving the rest to show up in the prompt as typed text.
    const POINTER_BACKLOG: usize = 256;
    let mut at = 0;
    while at < bytes.len() && !writer.backlog().is_some_and(|n| n > POINTER_BACKLOG) {
        let end = term::slice_end(bytes, at, term::SLICE);
        writer.write(&bytes[at..end])?;
        at = end;
    }
    Ok(())
}

fn closed() -> io::Error {
    io::Error::other("terminal input closed")
}
