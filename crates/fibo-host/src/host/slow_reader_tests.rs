use super::*;
use fibo_platform::local;

/// Another machine's window read its workspace through an ssh forward at about 54 KB/s;
/// the host gave up after 5 seconds without a byte taken, mid-reply, and that machine could
/// not be reached at all. A reader that pauses longer than that still gets the whole reply.
#[test]
fn a_reader_that_stalls_past_the_old_timeout_still_gets_the_whole_reply() {
    let (mut host, _root) = super::tests::fixture();
    // Far more than the socket buffers hold, so the host's write has to wait for the reader.
    host.workspace.spaces.push(fibo_core::model::Space {
        id: "big".into(),
        name: "x".repeat(4 << 20),
        cwd: String::new(),
    });
    let shared = Arc::new(Mutex::new(host));
    let (server, mut window) = local::Stream::pair().unwrap();
    thread::spawn(move || client::serve_client(server, shared, 9));
    window
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    send(&mut window, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    assert!(matches!(
        receive::<Reply>(&mut window).unwrap(),
        Reply::Hello { .. }
    ));
    send(&mut window, CONTROL, &Request::Workspace).unwrap();
    // The first blocked write returns at the timeout with what it took; only the next one,
    // taking nothing, cuts the reply: a pause past both.
    thread::sleep(Duration::from_secs(11));
    let Reply::Workspace(w) = receive::<Reply>(&mut window).expect("cut off mid-reply") else {
        panic!("not a workspace");
    };
    let big = w.spaces.iter().find(|s| s.id == "big").expect("the space");
    assert_eq!(big.name.len(), 4 << 20);
}
