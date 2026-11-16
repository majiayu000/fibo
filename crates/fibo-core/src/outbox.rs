//! What a pane types, on its way to the program: queued by bytes, not by request count.
//!
//! The window queues each key as it is pressed and the host reads them off the socket;
//! either side can fall behind for a moment (the host waiting on a lock, a PTY the program
//! is slow to drain). A queue counted in requests filled up after 128 keys or scroll
//! arrows and dropped the next ones ("输入队列已满"). Here consecutive input joins one
//! request, a newer size replaces a queued one, and only a backlog no hand can type
//! (`limit` bytes) is refused. Mouse reports are dropped quietly once anything is waiting:
//! the next one says where the pointer is.
use crate::protocol::Request;
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

/// Input joins the queued request up to this size; `protocol::send_input` frames at 16 KiB.
const JOIN: usize = 16 * 1024;
/// Mouse reports are dropped while this much input is still waiting.
const LOSSY_BACKLOG: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// More than `limit` bytes are waiting; this input was not queued.
    Full,
    /// The connection is gone; nothing queued will be sent.
    Closed,
}

pub struct Outbox {
    limit: usize,
    inner: Mutex<Inner>,
    ready: Condvar,
}
#[derive(Default)]
struct Inner {
    queue: VecDeque<Request>,
    bytes: usize,
    closed: bool,
}

impl Outbox {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            inner: Mutex::new(Inner::default()),
            ready: Condvar::new(),
        }
    }

    pub fn push(&self, request: Request) -> Result<(), Refused> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.closed {
            return Err(Refused::Closed);
        }
        match request {
            Request::Input { lossy: true, .. } if inner.bytes > LOSSY_BACKLOG => return Ok(()),
            Request::Input { bytes, lossy } => {
                if inner.bytes + bytes.len() > self.limit {
                    return Err(Refused::Full);
                }
                inner.bytes += bytes.len();
                match inner.queue.back_mut() {
                    Some(Request::Input {
                        bytes: queued,
                        lossy: queued_lossy,
                    }) if *queued_lossy == lossy && queued.len() + bytes.len() <= JOIN => {
                        queued.extend(bytes)
                    }
                    _ => inner.queue.push_back(Request::Input { bytes, lossy }),
                }
            }
            resize @ Request::Resize { .. } => match inner.queue.back_mut() {
                Some(queued @ Request::Resize { .. }) => *queued = resize,
                _ => inner.queue.push_back(resize),
            },
            other => inner.queue.push_back(other),
        }
        self.ready.notify_one();
        Ok(())
    }

    /// Everything queued, in order; waits while the queue is empty. `None` once closed
    /// and drained.
    pub fn take(&self) -> Option<Vec<Request>> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        while inner.queue.is_empty() {
            if inner.closed {
                return None;
            }
            inner = self.ready.wait(inner).unwrap_or_else(|e| e.into_inner());
        }
        inner.bytes = 0;
        Some(inner.queue.drain(..).collect())
    }

    /// Refuses further pushes; `take` still hands out what was already queued.
    pub fn close(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        self.ready.notify_all();
    }

    /// Drops what is queued as well: nothing will send it.
    pub fn abandon(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.closed = true;
        inner.queue.clear();
        inner.bytes = 0;
        self.ready.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(b: &[u8]) -> Request {
        Request::Input {
            bytes: b.to_vec(),
            lossy: false,
        }
    }

    #[test]
    fn a_thousand_keys_queued_behind_a_stalled_reader_all_arrive_in_order() {
        let outbox = Outbox::new(1 << 20);
        let typed: Vec<u8> = (0..1000).map(|i| b'a' + (i % 26) as u8).collect();
        for b in &typed {
            outbox.push(key(&[*b])).unwrap();
        }
        let batch = outbox.take().unwrap();
        let mut sent = Vec::new();
        for r in batch {
            let Request::Input { bytes, .. } = r else {
                panic!("not input")
            };
            sent.extend(bytes);
        }
        assert_eq!(sent, typed);
    }

    #[test]
    fn sizes_collapse_but_keep_their_place_between_inputs() {
        let outbox = Outbox::new(1 << 20);
        let size = |cols| Request::Resize {
            cols,
            rows: 24,
            cell_width: 0,
            cell_height: 0,
        };
        outbox.push(key(b"a")).unwrap();
        for cols in 80..120 {
            outbox.push(size(cols)).unwrap();
        }
        outbox.push(key(b"b")).unwrap();
        let batch = outbox.take().unwrap();
        assert_eq!(batch.len(), 3);
        assert!(matches!(batch[1], Request::Resize { cols: 119, .. }));
    }

    #[test]
    fn mouse_reports_never_join_keys_and_are_dropped_behind_a_backlog() {
        let outbox = Outbox::new(1 << 20);
        let mouse = || Request::Input {
            bytes: b"\x1b[<64;1;1M".to_vec(),
            lossy: true,
        };
        outbox.push(key(b"x")).unwrap();
        outbox.push(mouse()).unwrap();
        outbox.push(key(&[b'y'; LOSSY_BACKLOG])).unwrap();
        outbox.push(mouse()).unwrap();
        let batch = outbox.take().unwrap();
        assert_eq!(batch.len(), 3);
        assert!(matches!(batch[1], Request::Input { lossy: true, .. }));
    }

    #[test]
    fn only_a_backlog_past_the_limit_is_refused_and_close_says_so() {
        let outbox = Outbox::new(8);
        outbox.push(key(b"12345678")).unwrap();
        assert_eq!(outbox.push(key(b"9")), Err(Refused::Full));
        outbox.take().unwrap();
        outbox.push(key(b"9")).unwrap();
        outbox.close();
        assert_eq!(outbox.push(key(b"0")), Err(Refused::Closed));
        assert!(outbox.take().is_some());
        assert!(outbox.take().is_none());
    }
}
