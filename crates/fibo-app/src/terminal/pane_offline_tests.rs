use super::*;
use crate::connection::Connection;
use crate::terminal::terminal_input::{alternate_scroll, focus_bytes};
use fibo_core::protocol::*;
use fibo_core::terminal::Vt;
use fibo_platform::local;
use futures::StreamExt;
use std::sync::mpsc;

#[test]
fn what_is_kept_is_given_back_whole_and_in_order() {
    let mut kept = OfflineInput::default();
    assert!(kept.keep(b"half an "));
    assert!(kept.keep(b"answer"));
    assert_eq!(kept.take(), b"half an answer");
    assert!(kept.is_empty());
}

#[test]
fn past_the_limit_nothing_of_that_input_is_kept() {
    let mut kept = OfflineInput::default();
    assert!(kept.keep(&[b'a'; LIMIT - 1]));
    assert!(
        !kept.keep(b"bc"),
        "a line with a hole in it is worse than none"
    );
    assert!(kept.keep(b"d"));
    assert_eq!(kept.take().len(), LIMIT);
}

#[test]
fn the_offer_shows_keys_as_keys_and_hides_sequences() {
    let mut kept = OfflineInput::default();
    kept.keep("git sta\x1b[Dtus\x7f\x7fus\r改好了\t".as_bytes());
    assert_eq!(kept.preview(), "git status⌫⌫us⏎改好了⇥");
    let mut long = OfflineInput::default();
    long.keep(&[b'x'; 100]);
    assert_eq!(long.preview(), format!("…{}", "x".repeat(PREVIEW_CHARS)));
}

fn state(dropped: bool, vt: bool, writable: bool, ended: bool) -> State {
    State {
        vt: vt.then(|| Vt::new(80, 24).unwrap()),
        writable,
        error: None,
        cols: 80,
        rows: 24,
        socket: None,
        ended,
        dropped,
        scheme: None,
        cursor_shape: None,
    }
}

#[test]
fn input_is_kept_only_while_the_link_is_down_or_coming_back() {
    // Dropped, before or after the first screen.
    assert!(link_down(&state(true, true, false, false), false));
    assert!(link_down(&state(true, false, false, false), true));
    // Reattached, still waiting for the screen.
    assert!(link_down(&state(false, false, false, false), true));
    // A first connection still loading is not a drop: it says so as before.
    assert!(!link_down(&state(false, false, false, false), false));
    // Back with the screen and input; read-only is the takeover's business.
    assert!(!link_down(&state(false, true, true, false), true));
    assert!(!link_down(&state(false, true, false, false), false));
    // The program ended: nothing would ever take it.
    assert!(!link_down(&state(true, true, false, true), true));

    assert!(link_back(&state(false, true, true, false)));
    assert!(!link_back(&state(false, true, false, false)));
    assert!(!link_back(&state(false, false, true, false)));
    assert!(!link_back(&state(true, true, true, false)));
}

/// The wheel over less or man (alternate screen, 1007 on, no mouse tracking) becomes
/// arrow keys, and a focus change a report: both answer the screen shown now, which is
/// gone once the link is back. Kept, a few flicks filled the 4 KiB, hid in the offer as
/// an empty line, and went out as history steps on 发送.
#[test]
fn wheel_arrows_and_focus_reports_are_not_kept() {
    let mut vt = Vt::new(80, 24).unwrap();
    vt.feed(b"\x1b[?1049h\x1b[?1007h\x1b[?1004h");
    let arrows = alternate_scroll(&vt, true).unwrap().unwrap();
    let focus = focus_bytes(&vt, false).unwrap();
    let mut down = state(true, false, false, false);
    down.vt = Some(vt);
    let mut kept = OfflineInput::default();
    for _ in 0..100 {
        assert_eq!(kept.route(&arrows, false, &down, false), Route::Send);
    }
    assert_eq!(kept.route(&focus, false, &down, true), Route::Send);
    assert!(
        kept.is_empty(),
        "nothing the pane made up waits for the user"
    );
    // What the user types on the same screen is kept, and the room is still all there.
    assert_eq!(kept.route(&[b'a'; LIMIT], true, &down, false), Route::Kept);
    assert_eq!(kept.route(b"b", true, &down, false), Route::Full);
}

#[test]
fn the_offer_waits_for_the_link_and_for_something_kept() {
    let back = state(false, true, true, false);
    let mut kept = OfflineInput::default();
    assert_eq!(kept.offer(&back), None);
    assert_eq!(kept.route(b"ls\r", true, &back, false), Route::Send);
    assert_eq!(
        kept.offer(&back),
        None,
        "typed with the link up is sent, not kept"
    );
    let down = state(true, true, false, false);
    assert_eq!(kept.route(b"ls\r", true, &down, false), Route::Kept);
    assert_eq!(kept.offer(&down), None, "nowhere to send it yet");
    assert_eq!(kept.offer(&state(false, true, false, false)), None);
    assert_eq!(kept.offer(&back).as_deref(), Some("ls⏎"));
}

/// One life of a host: greets, attaches `id` writable, sends a blank screen; then hangs
/// up (`input` is None) or hands on the first input it is sent.
fn host_life(listener: &local::Listener, id: &str, input: Option<&mpsc::Sender<Vec<u8>>>) {
    let mut stream = listener.accept().unwrap();
    assert!(matches!(
        receive::<Request>(&mut stream).unwrap(),
        Request::Hello { .. }
    ));
    let hello = Reply::Hello {
        major: MAJOR,
        instance: "test".into(),
        build: Build::default(),
    };
    send(&mut stream, CONTROL, &hello).unwrap();
    assert!(matches!(
        receive::<Request>(&mut stream).unwrap(),
        Request::Attach { id: asked, .. } if asked == id
    ));
    let attached = Reply::Attached {
        writable: true,
        cols: 80,
        rows: 24,
    };
    send(&mut stream, CONTROL, &attached).unwrap();
    write_frame(
        &mut stream,
        SNAPSHOT,
        &Vt::new(80, 24).unwrap().snapshot().unwrap(),
    )
    .unwrap();
    write_frame(&mut stream, READY, &[]).unwrap();
    let Some(input) = input else {
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return;
    };
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    if let Request::Input { bytes, .. } = receive::<Request>(&mut stream).unwrap() {
        input.send(bytes).unwrap();
    }
}

fn wait_until(
    connection: &Connection,
    wake: &mut futures::channel::mpsc::UnboundedReceiver<()>,
    done: impl Fn(&State) -> bool,
) {
    while !done(&connection.state.lock().unwrap()) {
        futures::executor::block_on(wake.next()).expect("the link went quiet");
    }
}

/// #517: typed while the host was away, the half-written answer is kept, and once the
/// pane has attached again it reaches the program as it was typed.
#[test]
fn typed_while_disconnected_reaches_the_program_after_reconnecting() {
    let path = std::env::temp_dir().join(format!("fibo-offline-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = local::Listener::bind(&path).unwrap();
    let (got, arrived) = mpsc::channel();
    let host = std::thread::spawn(move || {
        host_life(&listener, "t", None);
        host_life(&listener, "t", Some(&got));
    });

    let (tx, mut wake) = futures::channel::mpsc::unbounded();
    let first = Connection::new(&path, "t", false, tx.clone());
    wait_until(&first, &mut wake, |s| s.dropped);
    // What the pane does with a key now: the link refuses it, so it is kept.
    let typed = "half an answer\r".as_bytes();
    assert!(
        first
            .send(Request::Input {
                bytes: typed.to_vec(),
                lossy: false
            })
            .is_err()
    );
    let mut kept = OfflineInput::default();
    let route = kept.route(typed, true, &first.state.lock().unwrap(), false);
    assert_eq!(route, Route::Kept);
    assert_eq!(kept.offer(&first.state.lock().unwrap()), None);

    let second = Connection::new(&path, "t", false, tx);
    wait_until(&second, &mut wake, link_back);
    let offer = kept.offer(&second.state.lock().unwrap());
    assert_eq!(offer.as_deref(), Some("half an answer⏎"));
    // 发送: what was kept goes out like a key now would.
    let bytes = kept.take();
    let route = kept.route(&bytes, true, &second.state.lock().unwrap(), false);
    assert_eq!(route, Route::Send);
    second
        .send(Request::Input {
            bytes,
            lossy: false,
        })
        .unwrap();
    let received = arrived
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the host never got the kept input");
    assert_eq!(received, typed);
    host.join().unwrap();
    drop((first, second));
    let _ = std::fs::remove_file(path);
}

/// Refused for room while the link was down, the warning went with that link; the
/// offer still says some was not kept, until what was kept is sent or dropped.
#[test]
fn the_offer_remembers_input_refused_for_room() {
    let mut kept = OfflineInput::default();
    assert!(kept.keep(&[b'a'; LIMIT]));
    assert!(!kept.overflowed());
    assert!(!kept.keep(b"b"));
    assert!(kept.overflowed());
    kept.clear();
    assert!(!kept.overflowed());
}

/// 发送 with the link's outbox full sent nothing: what was kept is offered again, in
/// front of anything typed since, and still says what was not kept.
#[test]
fn what_the_link_refused_is_given_back() {
    let mut kept = OfflineInput::default();
    kept.keep(&[b'a'; LIMIT]);
    kept.keep(b"b");
    let overflowed = kept.overflowed();
    let bytes = kept.take();
    assert!(!kept.overflowed());
    kept.keep(b"x");
    kept.give_back(bytes, overflowed);
    assert!(kept.overflowed());
    let back = kept.take();
    assert_eq!(back.len(), LIMIT + 1);
    assert_eq!(back.last(), Some(&b'x'));
}

/// Before the new screen arrives there is no terminal to ask for its modes; Backspace
/// and Enter are kept all the same, or `ab⌫c` would be offered as `abc`.
#[test]
fn keys_typed_before_the_screen_is_back_are_encoded() {
    use crate::terminal::keyboard::encode;
    use fibo_core::terminal::libghostty_vt::key::{Key, Mods};
    let none = Mods::empty();
    assert_eq!(
        encode(None, Key::Backspace, "backspace", none).unwrap(),
        b"\x7f"
    );
    assert_eq!(encode(None, Key::Enter, "enter", none).unwrap(), b"\r");
    assert_eq!(encode(None, Key::C, "c", Mods::CTRL).unwrap(), [3]);
}
