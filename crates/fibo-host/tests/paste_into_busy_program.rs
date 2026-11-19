//! A paste longer than the PTY's input queue (about 1 KiB) reaches a program that is busy
//! printing. Such a program, like Codex redrawing its spinner, reads input only between
//! redraws, and a redraw waits until the host has taken its output. The host used to type
//! while holding the terminal's lock, which its read pump also needs, so neither moved
//! until the write gave up after 5 seconds: "输入中止，已接受 1022 字节；禁止自动重发" under
//! a Codex pane, with the rest of the paste lost.
#[path = "support/tied.rs"]
mod tied;
use fibo_core::{model::*, protocol::*};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tied::SpawnTied;

/// Four times the input queue.
const PASTE: usize = 4000;

/// Redraws about 8 KB at a time with a blocking write, and reads its input only between
/// redraws, until it has the whole paste.
const BUSY: &str = r#"
import os, sys, tty, select
ready, out, done = sys.argv[1:4]
want = int(sys.argv[4])
tty.setraw(0)
open(ready, "w").close()
frame = b"\x1b[H" + (b"x" * 70 + b"\r\n") * 110
got = b""
while len(got) < want:
    os.write(1, frame)
    if select.select([0], [], [], 0.01)[0]:
        got += os.read(0, 65536)
open(out, "wb").write(got)
open(done, "w").close()
while True:
    os.write(1, frame)
"#;

struct Host {
    root: PathBuf,
    socket: PathBuf,
    _process: tied::Tied,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn rpc(socket: &Path, request: &Request) -> Reply {
    let mut stream = UnixStream::connect(socket).unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    receive::<Reply>(&mut stream).unwrap();
    send(&mut stream, CONTROL, request).unwrap();
    receive::<Reply>(&mut stream).unwrap()
}

fn wait_for(path: &Path, what: &str) {
    let until = Instant::now() + Duration::from_secs(20);
    while !path.exists() {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A host with one terminal running the busy program, which is in raw mode and printing.
fn busy_program() -> (Host, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("kl-pb-{}", uuid::Uuid::new_v4().simple()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let process = Command::new(env!("CARGO_BIN_EXE_fibo"))
        .args(["serve", "--runtime"])
        .arg(&root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    let socket = root.join("workbench.sock");
    wait_for(&socket, "host never listened");
    std::thread::sleep(Duration::from_millis(200));
    rpc(
        &socket,
        &Request::Edit {
            request: None,
            edit: Edit::AddSpace(Space {
                id: "s".into(),
                name: "Tests".into(),
                cwd: "/tmp".into(),
            }),
        },
    );
    let (ready, out, done) = (root.join("ready"), root.join("out"), root.join("done"));
    let argv = [
        "python3".to_owned(),
        "-c".to_owned(),
        BUSY.to_owned(),
        ready.display().to_string(),
        out.display().to_string(),
        done.display().to_string(),
        PASTE.to_string(),
    ];
    rpc(
        &socket,
        &Request::Launch {
            id: "t".into(),
            space: "s".into(),
            argv: argv.to_vec(),
            cwd: "/tmp".into(),
            cols: 80,
            rows: 24,
            scheme: Default::default(),
            colors: None,
            start: None,
        },
    );
    wait_for(&ready, "the program never went raw");
    // Let it fill its output queue first, as a redrawing Agent has.
    std::thread::sleep(Duration::from_millis(300));
    let host = Host {
        root,
        socket,
        _process: process,
    };
    (host, out, done)
}

fn paste() -> Vec<u8> {
    (0..PASTE).map(|i| b'a' + (i % 26) as u8).collect()
}

fn whole_paste_arrives(out: &Path, done: &Path, started: Instant) {
    wait_for(done, "the paste never reached the program");
    assert_eq!(
        std::fs::read(out).unwrap(),
        paste(),
        "paste cut or reordered"
    );
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(4),
        "the paste waited {took:?}: typing and the output it waits on held each other up"
    );
}

/// `fibo send`, the phone and another computer type this way.
#[test]
fn a_long_paste_into_a_program_busy_printing_arrives_whole() {
    let (host, out, done) = busy_program();
    let started = Instant::now();
    let reply = rpc(
        &host.socket,
        &Request::SendInput {
            id: "t".into(),
            bytes: paste(),
        },
    );
    assert!(matches!(reply, Reply::Ok), "{reply:?}");
    whole_paste_arrives(&out, &done, started);
}

/// The window's pane types this way.
#[test]
fn a_long_paste_from_an_attached_pane_into_a_busy_program_arrives_whole() {
    let (host, out, done) = busy_program();
    let mut stream = UnixStream::connect(&host.socket).unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    receive::<Reply>(&mut stream).unwrap();
    send(
        &mut stream,
        CONTROL,
        &Request::Attach {
            id: "t".into(),
            takeover: true,
            watch: false,
        },
    )
    .unwrap();
    assert!(matches!(
        receive::<Reply>(&mut stream).unwrap(),
        Reply::Attached { .. }
    ));
    let (acks, ack) = std::sync::mpsc::channel();
    let mut reader = stream.try_clone().unwrap();
    std::thread::spawn(move || {
        while let Ok((kind, bytes)) = read_frame(&mut reader) {
            if kind == ACK {
                let _ = acks.send(serde_json::from_slice::<Reply>(&bytes).ok());
            }
        }
    });
    let started = Instant::now();
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: paste(),
            lossy: false,
        },
    )
    .unwrap();
    let reply = ack.recv_timeout(Duration::from_secs(20)).unwrap();
    assert!(matches!(reply, Some(Reply::Ok)), "{reply:?}");
    whole_paste_arrives(&out, &done, started);
}
