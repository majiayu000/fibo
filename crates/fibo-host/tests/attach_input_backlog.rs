//! Keys a client sends while the program in the terminal is not reading are taken off the
//! socket at once and reach the program in order once it reads. The host used to read the
//! next key only after writing the last one into the PTY (and taking the host's lock):
//! a busy program or a held lock backed the window's 128-request queue up until it
//! dropped keys ("输入队列已满或连接已断开").
#[path = "support/tied.rs"]
mod tied;
use fibo_core::{model::*, protocol::*};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tied::SpawnTied;

fn rpc(socket: &Path, request: &Request) {
    let mut stream = UnixStream::connect(socket).unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    receive::<Reply>(&mut stream).unwrap();
    send(&mut stream, CONTROL, request).unwrap();
    receive::<Reply>(&mut stream).unwrap();
}

fn wait_for(path: &Path, what: &str) {
    let until = Instant::now() + Duration::from_secs(20);
    while !path.exists() {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn keys_sent_while_the_program_is_not_reading_never_back_up_the_client() {
    let root = std::env::temp_dir().join(format!("kl-ib-{}", uuid::Uuid::new_v4().simple()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let _host = Command::new(env!("CARGO_BIN_EXE_fibo"))
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
    // Raw mode and no reader for 3 seconds: the PTY's input queue (about 1 KiB) fills and
    // the host's write to it waits, as it does behind a busy program or a held lock.
    const KEYS: usize = 3000;
    let (raw, out, done) = (root.join("raw"), root.join("out"), root.join("done"));
    let script = format!(
        "stty raw -echo; : > '{}'; sleep 3; head -c {KEYS} > '{}'; : > '{}'; sleep 30",
        raw.display(),
        out.display(),
        done.display(),
    );
    rpc(
        &socket,
        &Request::Launch {
            id: "t".into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), script],
            cwd: "/tmp".into(),
            cols: 80,
            rows: 24,
            scheme: Default::default(),
            colors: None,
        },
    );
    wait_for(&raw, "the terminal never went raw");

    let mut stream = UnixStream::connect(&socket).unwrap();
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
    // Read the screen and acknowledgements away, as the window does.
    let mut reader = stream.try_clone().unwrap();
    std::thread::spawn(move || while read_frame(&mut reader).is_ok() {});
    // One frame per key, as the window sends typing.
    let typed: Vec<u8> = (0..KEYS).map(|i| b'a' + (i % 26) as u8).collect();
    let started = Instant::now();
    for key in &typed {
        send(
            &mut stream,
            CONTROL,
            &Request::Input {
                bytes: vec![*key],
                lossy: false,
            },
        )
        .unwrap();
    }
    let sending = started.elapsed();
    assert!(
        sending < Duration::from_millis(1500),
        "the host stopped reading keys while the program did not: {KEYS} took {sending:?}"
    );
    wait_for(&done, "the keys never reached the program");
    assert_eq!(
        std::fs::read(&out).unwrap(),
        typed,
        "keys lost or reordered"
    );
    let _ = std::fs::remove_dir_all(&root);
}
