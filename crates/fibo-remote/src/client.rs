//! A client of one machine over a byte pipe: usually a `fibo remote --stdio` child,
//! run here or through `ssh`. Requests are matched to replies by id; a stream's lines
//! go to the callback registered with the request that opened it.
//!
//! Callbacks run on the reader thread: they must not wait on this client (a request
//! from inside one would wait for the thread that is running it). Hand work to a
//! channel instead.
use crate::protocol::{Incoming, Op, Request, StreamBody};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Reply = Result<Value, String>;
type OnStream = Box<dyn FnMut(StreamBody) + Send>;

#[derive(Default)]
struct Shared {
    pending: HashMap<u64, Sender<Reply>>,
    streams: HashMap<u64, OnStream>,
    /// Why the pipe ended; set once, then every request fails with it.
    ended: Option<String>,
}

pub struct Client {
    writer: Mutex<Box<dyn Write + Send>>,
    next: AtomicU64,
    shared: Arc<Mutex<Shared>>,
    child: Mutex<Option<Child>>,
}

impl Client {
    /// Runs `argv` and speaks the protocol on its stdin and stdout. The last line it
    /// writes to stderr becomes the reason when it ends (ssh's own errors land there).
    pub fn spawn(argv: &[String]) -> io::Result<Self> {
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| io::Error::other("empty command"))?;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let errors = std::thread::spawn(move || {
            let mut last = None;
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    last = Some(line.trim().to_owned());
                }
            }
            last
        });
        let client = Self::start(stdout, stdin, Some(errors));
        *client.child.lock().unwrap() = Some(child);
        Ok(client)
    }

    /// Speaks the protocol on an existing pair, e.g. both ends of a test pipe.
    pub fn new(read: impl Read + Send + 'static, write: impl Write + Send + 'static) -> Self {
        Self::start(read, write, None)
    }

    fn start(
        read: impl Read + Send + 'static,
        write: impl Write + Send + 'static,
        stderr: Option<std::thread::JoinHandle<Option<String>>>,
    ) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let reader = Arc::clone(&shared);
        std::thread::spawn(move || {
            let mut reason = "连接断开".to_owned();
            for line in BufReader::new(read).lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Incoming>(&line) {
                    Ok(Incoming::Reply { id, ok }) => reply(&reader, id, Ok(ok)),
                    Ok(Incoming::Failed { id, error }) => reply(&reader, id, Err(error)),
                    Ok(Incoming::Stream(m)) => {
                        let end = matches!(m.body, StreamBody::End { .. });
                        // Taken out while it runs, so the callback may close streams.
                        let callback = reader.lock().unwrap().streams.remove(&m.stream);
                        if let Some(mut callback) = callback {
                            callback(m.body);
                            if !end {
                                reader.lock().unwrap().streams.insert(m.stream, callback);
                            }
                        }
                    }
                    Ok(Incoming::Error { error }) => {
                        reason = error;
                        break;
                    }
                    Err(e) => {
                        reason = format!("对方说的不是远程协议：{e}");
                        break;
                    }
                }
            }
            // The child is gone once its stdout ends; its stderr says why.
            if let Some(why) = stderr.and_then(|h| h.join().ok().flatten()) {
                reason = why;
            }
            let (pending, streams) = {
                let mut s = reader.lock().unwrap();
                s.ended.get_or_insert(reason.clone());
                (
                    std::mem::take(&mut s.pending),
                    std::mem::take(&mut s.streams),
                )
            };
            for (_, tx) in pending {
                let _ = tx.send(Err(reason.clone()));
            }
            for (_, mut callback) in streams {
                callback(StreamBody::End {
                    reason: reason.clone(),
                });
            }
        });
        Self {
            writer: Mutex::new(Box::new(write)),
            next: AtomicU64::new(1),
            shared,
            child: Mutex::new(None),
        }
    }

    /// Sends `op` and waits for its reply.
    pub fn request(&self, op: Op, timeout: Duration) -> io::Result<Value> {
        self.call(op, None, timeout).map(|(_, ok)| ok)
    }

    /// Opens a stream (`events` or `watch`): `on` gets each of its lines, the last one
    /// an `End`. Returns the stream's id and the reply that opened it.
    pub fn stream(
        &self,
        op: Op,
        on: impl FnMut(StreamBody) + Send + 'static,
        timeout: Duration,
    ) -> io::Result<(u64, Value)> {
        self.call(op, Some(Box::new(on)), timeout)
    }

    /// Ends a stream; its callback is dropped without an `End`.
    pub fn close(&self, stream: u64) {
        self.shared.lock().unwrap().streams.remove(&stream);
        let _ = self.send(Request {
            id: self.next.fetch_add(1, Ordering::Relaxed),
            op: Op::Close { stream },
        });
    }

    /// Why the pipe ended, once it has.
    pub fn ended(&self) -> Option<String> {
        self.shared.lock().unwrap().ended.clone()
    }

    fn call(&self, op: Op, on: Option<OnStream>, timeout: Duration) -> io::Result<(u64, Value)> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        {
            let mut s = self.shared.lock().unwrap();
            if let Some(why) = &s.ended {
                return Err(io::Error::other(why.clone()));
            }
            s.pending.insert(id, tx);
            if let Some(on) = on {
                s.streams.insert(id, on);
            }
        }
        let forget = |s: &Mutex<Shared>| {
            let mut s = s.lock().unwrap();
            s.pending.remove(&id);
            s.streams.remove(&id);
        };
        if let Err(e) = self.send(Request { id, op }) {
            forget(&self.shared);
            return Err(e);
        }
        match rx.recv_timeout(timeout) {
            Ok(Ok(ok)) => Ok((id, ok)),
            Ok(Err(error)) => {
                forget(&self.shared);
                Err(io::Error::other(error))
            }
            Err(_) => {
                forget(&self.shared);
                Err(io::Error::new(io::ErrorKind::TimedOut, "对方没有回复"))
            }
        }
    }

    fn send(&self, request: Request) -> io::Result<()> {
        let mut line = serde_json::to_vec(&request).map_err(io::Error::other)?;
        line.push(b'\n');
        let mut w = self.writer.lock().unwrap();
        w.write_all(&line)?;
        w.flush()
    }
}

fn reply(shared: &Mutex<Shared>, id: u64, result: Reply) {
    if let Some(tx) = shared.lock().unwrap().pending.remove(&id) {
        let _ = tx.send(result);
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};

    /// A fake machine on the other end of a loopback connection, which every platform has:
    /// answers each request with `answer(request)`, a list of lines to write back.
    fn machine(answer: impl Fn(&Value) -> Vec<Value> + Send + 'static) -> Client {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let ours = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (theirs, _) = listener.accept().unwrap();
        let mut out = theirs.try_clone().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(theirs).lines().map_while(Result::ok) {
                let request: Value = serde_json::from_str(&line).unwrap();
                for reply in answer(&request) {
                    writeln!(out, "{reply}").unwrap();
                }
            }
        });
        Client::new(ours.try_clone().unwrap(), ours)
    }

    #[test]
    fn replies_reach_the_request_that_asked() {
        let c = machine(|r| vec![json!({ "id": r["id"], "ok": { "op": r["op"] } })]);
        let t = Duration::from_secs(5);
        assert_eq!(c.request(Op::List, t).unwrap(), json!({ "op": "list" }));
        let err = machine(|r| vec![json!({ "id": r["id"], "error": "不认识" })])
            .request(Op::List, t)
            .unwrap_err();
        assert_eq!(err.to_string(), "不认识");
    }

    #[test]
    fn a_stream_gets_its_lines_and_the_end_when_the_pipe_closes() {
        let c = machine(|r| {
            let id = r["id"].clone();
            if r["op"] == "watch" {
                vec![
                    json!({ "id": id, "ok": { "stream": id, "cols": 80, "rows": 24 } }),
                    json!({ "stream": id, "type": "output", "vt": "aGk=" }),
                ]
            } else {
                vec![]
            }
        });
        let (tx, rx) = channel();
        let (stream, ok) = c
            .stream(
                Op::Watch {
                    terminal: "t".into(),
                    take: false,
                },
                move |body| tx.send(body).unwrap(),
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(ok["cols"], 80);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            StreamBody::Output { vt: "aGk=".into() }
        );
        assert!(stream > 0);
        drop(c);
    }

    #[test]
    fn a_dead_child_fails_waiting_requests_with_its_last_error() {
        #[cfg(unix)]
        let argv = [
            "sh".to_owned(),
            "-c".to_owned(),
            "read line; echo 'ssh: connect to host mac2: Connection refused' >&2".to_owned(),
        ];
        #[cfg(windows)]
        let argv = [
            "cmd".to_owned(),
            "/c".to_owned(),
            "echo ssh: connect to host mac2: Connection refused 1>&2".to_owned(),
        ];
        let c = Client::spawn(&argv).unwrap();
        let err = c.request(Op::List, Duration::from_secs(5)).unwrap_err();
        assert!(err.to_string().contains("Connection refused"), "{err}");
        assert!(c.ended().unwrap().contains("Connection refused"));
        assert!(c.request(Op::List, Duration::from_secs(5)).is_err());
    }

    #[test]
    fn a_silent_machine_times_out() {
        let c = machine(|_| vec![]);
        let err = c.request(Op::List, Duration::from_millis(100)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    }
}
