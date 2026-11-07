//! What a client sends right after attaching reaches the terminal, even while the first
//! screen is still on its way. A client that has the attach reply, or has just read READY,
//! may type and resize at once; nothing tells it to wait longer, and the host's refusal only
//! comes back as an acknowledgement most callers do not read.
#[path = "support/tied.rs"]
mod tied;
use mooring_core::{model::*, protocol::*};
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
fn input_and_resize_sent_before_the_first_screen_arrives_are_kept() {
    let root = std::env::temp_dir().join(format!("kl-ea-{}", uuid::Uuid::new_v4().simple()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let _host = Command::new(env!("CARGO_BIN_EXE_mooring"))
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
    // Enough history that the first screen does not fit in the socket's buffer: the host
    // cannot finish sending it until this client reads, and this client does not.
    let (filled, size) = (root.join("filled"), root.join("size"));
    let script = format!(
        "yes 0123456789012345678901234567890123456789 | head -n 40000; : > '{}'; read _; stty size > '{}'; sleep 30",
        filled.display(),
        size.display(),
    );
    rpc(
        &socket,
        &Request::Launch {
            id: "t".into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), script],
            cwd: "/tmp".into(),
            cols: 120,
            rows: 30,
            scheme: Default::default(),
            colors: None,
        },
    );
    wait_for(&filled, "the terminal never filled its history");
    std::thread::sleep(Duration::from_millis(300));

    let mut stream = UnixStream::connect(&socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
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
    send(
        &mut stream,
        CONTROL,
        &Request::Resize {
            cols: 132,
            rows: 40,
            cell_width: 0,
            cell_height: 0,
        },
    )
    .unwrap();
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: b"go\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();

    wait_for(&size, "the input never reached the terminal");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(std::fs::read_to_string(&size).unwrap().trim(), "40 132");
    let _ = std::fs::remove_dir_all(&root);
}
