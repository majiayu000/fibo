//! Typing into a terminal without holding it.
//!
//! A terminal's input queue holds about 1 KiB, and a program busy printing (Codex redrawing
//! its spinner) reads more only once its own output has been taken. Taking that output needs
//! the terminal's lock, so typing under that lock left both waiting until the write gave up
//! after five seconds, with a paste cut in the middle. Here one thread per terminal types, in
//! the order asked, and whoever asked waits with no lock held.
use std::io;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Instant;

use super::lock;
use fibo_platform::terminal as term;

type Done = mpsc::SyncSender<io::Result<()>>;

struct Write {
    bytes: Vec<u8>,
    pointer: bool,
    done: Option<Done>,
}

const REPLY_BUDGET: usize = 64 * 1024;

#[derive(Default)]
struct Queued {
    pending: usize,
    reply_bytes: usize,
    paused: bool,
}

#[derive(Default)]
struct Queue {
    state: Mutex<Queued>,
    idle: Condvar,
}

impl Queue {
    fn completed(&self, reply_bytes: usize) -> io::Result<()> {
        let mut state = lock(&self.state)?;
        state.pending -= 1;
        state.reply_bytes -= reply_bytes;
        if state.pending == 0 {
            self.idle.notify_all();
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct Input {
    sender: mpsc::Sender<Write>,
    queue: Arc<Queue>,
}

/// A quiescent writer stays idle while the terminal snapshot is handed over.
pub(super) struct InputPause(Arc<Queue>);

impl Drop for InputPause {
    fn drop(&mut self) {
        if let Ok(mut state) = lock(&self.0.state) {
            state.paused = false;
        }
    }
}

impl Input {
    fn channel() -> (Self, mpsc::Receiver<Write>) {
        let (sender, receiver) = mpsc::channel();
        let input = Self {
            sender,
            queue: Arc::new(Queue::default()),
        };
        (input, receiver)
    }

    /// Its thread ends once every copy is dropped and what was queued has been typed.
    pub(super) fn start(master: &term::Master, id: &str) -> io::Result<Self> {
        let mut writer = master.writer()?;
        let (input, rx) = Self::channel();
        let queue = input.queue.clone();
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
                    let reply_bytes = if done.is_none() { bytes.len() } else { 0 };
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
                    if queue.completed(reply_bytes).is_err() {
                        break;
                    }
                }
            })?;
        Ok(input)
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
        self.enqueue(Write {
            bytes,
            pointer,
            done: Some(done),
        })?;
        wait.recv().map_err(|_| closed())?
    }

    /// Answers the program's own query, after any typing already queued, without waiting:
    /// the read pump calls this and must go back to reading.
    pub(super) fn reply(&self, bytes: Vec<u8>) {
        if !bytes.is_empty() {
            let _ = self.enqueue(Write {
                bytes,
                pointer: false,
                done: None,
            });
        }
    }

    fn enqueue(&self, write: Write) -> io::Result<()> {
        let mut state = lock(&self.queue.state)?;
        if state.paused {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                t!("后台正在交接，请稍后再发送"),
            ));
        }
        let reply_bytes = if write.done.is_none() {
            write.bytes.len()
        } else {
            0
        };
        // A program that does not take its replies cannot make the read pump queue
        // them forever. Count a reply until its write completes, including any stall.
        if reply_bytes > REPLY_BUDGET - state.reply_bytes {
            return Ok(());
        }
        self.sender.send(write).map_err(|_| closed())?;
        state.pending += 1;
        state.reply_bytes += reply_bytes;
        Ok(())
    }

    pub(super) fn pause_if_idle(&self) -> io::Result<Option<InputPause>> {
        let mut state = lock(&self.queue.state)?;
        if state.pending != 0 || state.paused {
            return Ok(None);
        }
        state.paused = true;
        Ok(Some(InputPause(self.queue.clone())))
    }

    /// Wait with the host and terminal locks released, so the read pump can drain output.
    pub(super) fn wait_until_idle(&self, deadline: Instant) -> io::Result<()> {
        let wait = deadline.saturating_duration_since(Instant::now());
        let (state, _) = self
            .queue
            .idle
            .wait_timeout_while(lock(&self.queue.state)?, wait, |state| state.pending != 0)
            .map_err(|_| closed())?;
        if state.pending != 0 || Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                t!("仍有终端输入待写，交接已取消"),
            ));
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_budget_counts_the_active_write_and_never_limits_typing() {
        let (input, rx) = Input::channel();
        input.reply(vec![0; REPLY_BUDGET]);
        let reply = rx.try_recv().unwrap();
        assert!(reply.done.is_none());
        input.reply(vec![1]);
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));

        let (done, _) = mpsc::sync_channel(1);
        input
            .enqueue(Write {
                bytes: vec![2],
                pointer: true,
                done: Some(done),
            })
            .unwrap();
        let pointer = rx.try_recv().unwrap();
        assert!(pointer.pointer);
        assert_eq!(pointer.bytes, [2]);
        input.queue.completed(reply.bytes.len()).unwrap();
        input.reply(vec![3]);
        assert_eq!(rx.try_recv().unwrap().bytes, [3]);
    }

    #[test]
    fn handoff_pause_requires_an_empty_queue_and_ends_when_dropped() {
        let (input, rx) = Input::channel();
        input.reply(vec![1]);
        assert!(input.pause_if_idle().unwrap().is_none());
        input
            .queue
            .completed(rx.try_recv().unwrap().bytes.len())
            .unwrap();

        let paused = input.pause_if_idle().unwrap().unwrap();
        let (done, _) = mpsc::sync_channel(1);
        let rejected = input
            .enqueue(Write {
                bytes: vec![2],
                pointer: false,
                done: Some(done),
            })
            .unwrap_err();
        assert_eq!(rejected.kind(), io::ErrorKind::WouldBlock);
        drop(paused);
        input.reply(vec![3]);
        assert_eq!(rx.try_recv().unwrap().bytes, [3]);
    }
}
