//! Another desktop typing into this machine's terminal over `mooring remote` (multi-machine
//! step 2, docs/multi-machine/spec.md): raw input lands once, and its echo comes back fast.
use base64::Engine;
use mooring_core::{model::*, protocol::*};
use mooring_remote::client::Client;
use mooring_remote::protocol::{Hello, Op, StreamBody};
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const T: Duration = Duration::from_secs(10);

/// A host of its own with one terminal running `script`.
struct Machine {
    child: Child,
    root: PathBuf,
}
impl Machine {
    fn start(script: &str) -> Self {
        let root = std::env::temp_dir().join(format!("kl-rt-{}", uuid::Uuid::new_v4().simple()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_mooring"))
            .args(["serve", "--runtime"])
            .arg(&root)
            .env("MOORING_REMOTE_EXE", env!("CARGO_BIN_EXE_mooring"))
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let socket = root.join("workbench.sock");
        let until = Instant::now() + Duration::from_secs(30);
        while rpc(&socket, &Request::Workspace).is_err() {
            assert!(Instant::now() < until, "host never listened");
            std::thread::sleep(Duration::from_millis(20));
        }
        let space = Edit::AddSpace(Space {
            id: "s".into(),
            name: "Tests".into(),
            cwd: "/tmp".into(),
        });
        rpc(
            &socket,
            &Request::Edit {
                request: None,
                edit: space,
            },
        )
        .unwrap();
        let launch = Request::Launch {
            id: "far".into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            cwd: "/tmp".into(),
            cols: 120,
            rows: 30,
            scheme: Default::default(),
        };
        rpc(&socket, &launch).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        Self { child, root }
    }
    fn remote(&self) -> Vec<String> {
        vec![
            env!("CARGO_BIN_EXE_mooring").into(),
            "remote".into(),
            "--stdio".into(),
            "--runtime".into(),
            self.root.to_string_lossy().into_owned(),
        ]
    }
}
impl Drop for Machine {
    fn drop(&mut self) {
        let _ = rpc(
            &self.root.join("workbench.sock"),
            &Request::Stop { id: "far".into() },
        );
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn connect(argv: &[String]) -> Client {
    let c = Client::spawn(argv).unwrap();
    let hello = Op::Hello {
        version: Some(1),
        client: Some("desk-1".into()),
    };
    let hello: Hello = serde_json::from_value(c.request(hello, T).unwrap()).unwrap();
    assert!(hello.ops.iter().any(|o| o == "input"), "{:?}", hello.ops);
    c
}
fn input(text: &str, request: Option<&str>) -> Op {
    Op::Input {
        terminal: "far".into(),
        data: base64::engine::general_purpose::STANDARD.encode(text),
        request: request.map(str::to_owned),
    }
}

#[test]
fn typed_input_lands_once_even_when_sent_again() {
    let typed = std::env::temp_dir().join(format!("kl-typed-{}", uuid::Uuid::new_v4().simple()));
    let m = Machine::start(&format!("stty -echo; exec cat >> '{}'", typed.display()));
    let first = connect(&m.remote());
    first.request(input("one\r", Some("k1")), T).unwrap();
    drop(first);
    let again = connect(&m.remote());
    again.request(input("one\r", Some("k1")), T).unwrap();
    // Arrows and control keys go as the emulator encoded them.
    again.request(input("two\x1b[D\r", Some("k2")), T).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let text = std::fs::read_to_string(&typed).unwrap();
    let _ = std::fs::remove_file(&typed);
    assert_eq!(text.matches("one").count(), 1, "{text:?}");
    assert!(text.contains("two\x1b[D"), "{text:?}");
    let empty = again.request(input("", None), T).unwrap_err();
    assert!(empty.to_string().contains("1 字节"), "{empty}");
}

/// Key to echo, measured: `MOORING_LATENCY_CMD` (a JSON argv that speaks the remote
/// protocol to a machine with a terminal `far` running `cat`, e.g. over ssh) or, without
/// it, a host on this machine. Prints p50/p95/max; `MOORING_LATENCY_P95_MS` asserts.
#[test]
#[ignore = "measurement: cargo test -p mooring-host --test remote_typing -- --ignored --nocapture"]
fn key_echo_latency() {
    let local;
    let argv: Vec<String> = match std::env::var("MOORING_LATENCY_CMD") {
        Ok(json) => serde_json::from_str(&json).unwrap(),
        Err(_) => {
            local = Machine::start("exec cat > /dev/null");
            local.remote()
        }
    };
    let client = connect(&argv);
    let seen = Arc::new((Mutex::new(String::new()), Condvar::new()));
    let on = {
        let seen = Arc::clone(&seen);
        move |body: StreamBody| {
            let b64 = base64::engine::general_purpose::STANDARD;
            let bytes = match body {
                StreamBody::Screen { vt, .. } | StreamBody::Output { vt } => {
                    b64.decode(vt).unwrap_or_default()
                }
                _ => return,
            };
            seen.0
                .lock()
                .unwrap()
                .push_str(&String::from_utf8_lossy(&bytes));
            seen.1.notify_all();
        }
    };
    client
        .stream(
            Op::Watch {
                terminal: "far".into(),
                take: false,
            },
            on,
            T,
        )
        .unwrap();
    let mut took = Vec::new();
    for i in 0..100 {
        let marker = format!("q{i}z");
        let at = Instant::now();
        client
            .request(
                input(&marker, Some(&format!("lat-{i}-{}", std::process::id()))),
                T,
            )
            .unwrap();
        let (lock, wait) = &*seen;
        let mut text = lock.lock().unwrap();
        while !text.contains(&marker) {
            let (t, timeout) = wait.wait_timeout(text, Duration::from_secs(5)).unwrap();
            text = t;
            assert!(!timeout.timed_out(), "no echo of {marker}");
        }
        text.clear();
        drop(text);
        took.push(at.elapsed().as_secs_f64() * 1000.0);
        if i % 20 == 19 {
            // Ctrl-U: keep cat's line short.
            client.request(input("\x15", None), T).unwrap();
        }
    }
    took.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (p50, p95, max) = (took[49], took[94], took[99]);
    println!("LATENCY key→echo over {argv:?}: p50 {p50:.1} ms, p95 {p95:.1} ms, max {max:.1} ms");
    if let Ok(limit) = std::env::var("MOORING_LATENCY_P95_MS") {
        assert!(p95 <= limit.parse::<f64>().unwrap(), "p95 {p95:.1} ms");
    }
}

/// Step 3: a `watch` that takes the terminal over sizes it with `resize`, and the
/// program sees the new size; a stream that only looks may not.
#[test]
fn a_taken_terminal_follows_the_remote_panes_size() {
    let m = Machine::start("trap 'stty size' WINCH; stty size; while :; do sleep 0.05; done");
    let c = connect(&m.remote());
    let seen = Arc::new((Mutex::new(String::new()), Condvar::new()));
    let watch = |take: bool| {
        let seen = Arc::clone(&seen);
        let on = move |body: StreamBody| {
            let b64 = base64::engine::general_purpose::STANDARD;
            if let StreamBody::Output { vt } | StreamBody::Screen { vt, .. } = body {
                let bytes = b64.decode(vt).unwrap_or_default();
                seen.0
                    .lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&bytes));
                seen.1.notify_all();
            }
        };
        let op = Op::Watch {
            terminal: "far".into(),
            take,
        };
        c.stream(op, on, T).unwrap().0
    };
    let looking = watch(false);
    let refused = c.request(
        Op::Resize {
            stream: looking,
            cols: 132,
            rows: 40,
        },
        T,
    );
    assert!(refused.unwrap_err().to_string().contains("没有接管"));
    let taken = watch(true);
    c.request(
        Op::Resize {
            stream: taken,
            cols: 132,
            rows: 40,
        },
        T,
    )
    .unwrap();
    let (lock, wait) = &*seen;
    let mut text = lock.lock().unwrap();
    while !text.contains("40 132") {
        let (t, timeout) = wait.wait_timeout(text, Duration::from_secs(5)).unwrap();
        text = t;
        assert!(
            !timeout.timed_out(),
            "the program never saw 40x132: {:?}",
            *text
        );
    }
}
