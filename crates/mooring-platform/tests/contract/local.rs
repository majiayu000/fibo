use mooring_platform::local::{Listener, Stream};
use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::time::{Duration, Instant};

fn joined(name: &str) -> (Stream, Stream, std::path::PathBuf) {
    let path = crate::scratch(name).join("s.sock");
    let listener = Listener::bind(&path).unwrap();
    let client = Stream::connect(&path).unwrap();
    let server = listener.accept().unwrap();
    (client, server, path)
}

#[test]
fn bytes_go_both_ways_at_a_path() {
    let (mut a, mut b, _) = joined("local-both");
    a.write_all(b"hello").unwrap();
    let mut got = [0; 5];
    b.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"hello");
    b.write_all(b"there").unwrap();
    a.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"there");
}

#[test]
fn a_pair_is_joined() {
    let (mut a, mut b) = Stream::pair().unwrap();
    a.write_all(b"x").unwrap();
    let mut got = [0; 1];
    b.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"x");
}

#[test]
fn nobody_at_a_path_is_an_error_and_a_taken_path_cannot_be_bound() {
    let dir = crate::scratch("local-nobody");
    assert!(Stream::connect(dir.join("no.sock")).is_err());
    let path = dir.join("s.sock");
    let _first = Listener::bind(&path).unwrap();
    assert!(Listener::bind(&path).is_err());
}

#[test]
fn one_copy_reads_while_another_writes() {
    let (a, mut b, _) = joined("local-clone");
    let mut reader = a.try_clone().unwrap();
    let waiting = std::thread::spawn(move || {
        let mut got = [0; 4];
        reader.read_exact(&mut got).unwrap();
        got
    });
    // The read above is blocked, and a write on the other copy still goes through.
    std::thread::sleep(Duration::from_millis(100));
    let mut writer = a.try_clone().unwrap();
    writer.write_all(b"ping").unwrap();
    let mut got = [0; 4];
    b.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"ping");
    b.write_all(b"pong").unwrap();
    assert_eq!(&waiting.join().unwrap(), b"pong");
}

#[test]
fn a_read_with_a_timeout_gives_up_with_would_block() {
    let (mut a, _b, _) = joined("local-timeout");
    a.set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let at = Instant::now();
    let mut got = [0; 1];
    let error = a.read(&mut got).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock, "{error:?}");
    assert!(
        at.elapsed() >= Duration::from_millis(50),
        "{:?}",
        at.elapsed()
    );
    assert!(at.elapsed() < Duration::from_secs(5), "{:?}", at.elapsed());
}

#[test]
fn shutdown_wakes_a_read_waiting_in_another_thread() {
    let (a, _b, _) = joined("local-shutdown");
    let mut reader = a.try_clone().unwrap();
    let waiting = std::thread::spawn(move || {
        let mut got = [0; 1];
        reader.read(&mut got)
    });
    std::thread::sleep(Duration::from_millis(100));
    a.shutdown(Shutdown::Both).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while !waiting.is_finished() {
        assert!(Instant::now() < until, "the read never woke");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        waiting.join().unwrap().unwrap(),
        0,
        "the end of the connection"
    );
}

#[test]
fn is_open_tells_a_gone_peer_and_consumes_nothing() {
    let (a, mut b, _) = joined("local-open");
    assert!(a.is_open(), "quiet but there");
    b.write_all(b"z").unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !a.wait_readable(Duration::from_millis(50)).unwrap() {
        assert!(Instant::now() < until);
    }
    assert!(a.is_open(), "with something to read");
    let mut first = a.try_clone().unwrap();
    let mut got = [0; 1];
    first.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"z", "the look left it there");
    drop(b);
    let until = Instant::now() + Duration::from_secs(5);
    while a.is_open() {
        assert!(Instant::now() < until, "still open after its peer went");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn wait_readable_sleeps_for_as_long_as_asked_and_wakes_when_the_peer_goes() {
    let (a, b, _) = joined("local-wait");
    let at = Instant::now();
    assert!(!a.wait_readable(Duration::from_millis(150)).unwrap());
    assert!(
        at.elapsed() >= Duration::from_millis(100),
        "{:?}",
        at.elapsed()
    );
    drop(b);
    assert!(a.wait_readable(Duration::from_secs(5)).unwrap());
}

#[test]
fn the_other_end_is_this_account() {
    let (a, b, _) = joined("local-peer");
    assert!(a.peer_is_this_account().unwrap());
    assert!(b.peer_is_this_account().unwrap());
}

/// More than any buffer between the two holds, in the frame size the host writes.
#[test]
fn a_large_transfer_arrives_whole_and_in_order() {
    let (mut a, mut b, _) = joined("local-large");
    let total = 16 << 20;
    let writer = std::thread::spawn(move || {
        let chunk: Vec<u8> = (0..64 * 1024).map(|i| (i % 251) as u8).collect();
        let mut sent = 0;
        while sent < total {
            a.write_all(&chunk).unwrap();
            sent += chunk.len();
        }
    });
    let at = Instant::now();
    let mut got = 0usize;
    let mut buffer = vec![0; 64 * 1024];
    while got < total {
        let n = b.read(&mut buffer).unwrap();
        assert!(n > 0);
        for (i, byte) in buffer[..n].iter().enumerate() {
            assert_eq!(*byte, ((got + i) % (64 * 1024) % 251) as u8);
        }
        got += n;
    }
    writer.join().unwrap();
    eprintln!("16 MiB in {:?}", at.elapsed());
}

#[test]
fn a_listener_accepts_from_several_clients() {
    let path = crate::scratch("local-several").join("s.sock");
    let listener = Listener::bind(&path).unwrap();
    let clients: Vec<_> = (0..8u8)
        .map(|i| {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut stream = Stream::connect(&path).unwrap();
                stream.write_all(&[i]).unwrap();
            })
        })
        .collect();
    let mut seen = vec![];
    for _ in 0..8 {
        let mut stream = listener.accept().unwrap();
        let mut got = [0; 1];
        stream.read_exact(&mut got).unwrap();
        seen.push(got[0]);
    }
    for client in clients {
        client.join().unwrap();
    }
    seen.sort_unstable();
    assert_eq!(seen, (0..8).collect::<Vec<_>>());
}

#[test]
fn what_is_at_a_path_tells_a_socket_from_anything_else() {
    use mooring_platform::local::{AtPath, at_path, restrict};
    let dir = crate::scratch("local-at");
    let path = dir.join("s.sock");
    assert_eq!(at_path(&path).unwrap(), AtPath::Nothing);
    let _listener = Listener::bind(&path).unwrap();
    restrict(&path).unwrap();
    assert_eq!(at_path(&path).unwrap(), AtPath::MySocket);
    let file = dir.join("file");
    std::fs::write(&file, "x").unwrap();
    assert_eq!(at_path(&file).unwrap(), AtPath::Other);
    assert_eq!(at_path(&dir).unwrap(), AtPath::Other);
}

/// How the host publishes itself: listening at a temporary name, then moved into place, so a
/// client that sees the name can connect.
#[test]
fn a_socket_moved_into_place_still_takes_connections() {
    let dir = crate::scratch("local-moved");
    let (pending, socket) = (dir.join(".start"), dir.join("s.sock"));
    let listener = Listener::bind(&pending).unwrap();
    std::fs::rename(&pending, &socket).unwrap();
    let mut client = Stream::connect(&socket).unwrap();
    let mut server = listener.accept().unwrap();
    client.write_all(b"y").unwrap();
    let mut got = [0; 1];
    server.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"y");
    // A stale one left behind goes away with an ordinary remove.
    drop((client, server, listener));
    std::fs::remove_file(&socket).unwrap();
    assert!(!socket.exists());
}

/// A host that never ran has no folder yet. Whoever connects starts one on this, so it must not
/// read as some other failure.
#[test]
fn connecting_where_nothing_was_ever_made_is_not_found() {
    let dir = crate::scratch("local-missing");
    let in_a_folder = dir.join("s.sock");
    let without_a_folder = dir.join("not-there").join("s.sock");
    for path in [in_a_folder, without_a_folder] {
        let error = Stream::connect(&path).err().expect("nothing listens");
        assert!(
            matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ),
            "{path:?}: {error:?}"
        );
    }
}
