use super::Feed;
use crate::host::brief::Understood;
use fibo_core::model::Workspace;
use fibo_core::protocol::{CONTROL, MAJOR, RecognitionStatus, Reply, Request, receive, send};
use fibo_core::workspace_delta::{Index, diff};
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A host that counts the workspaces it is asked for and takes `slow` over each, the
/// snapshot and diff a big workspace costs. `brief: false` is a host from before `Brief`;
/// `refuse` answers with an error instead.
struct Fake {
    now: Mutex<Workspace>,
    sent: Mutex<Vec<Index>>,
    asked: Mutex<usize>,
    refuse: Mutex<bool>,
}
fn serve(name: &str, brief: bool, slow: Duration) -> (std::path::PathBuf, Arc<Fake>) {
    let dir = std::env::temp_dir().join(format!("fibo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let socket = dir.join("workbench.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let mut now = fibo_core::sample::every_state();
    now.revision = 7;
    let fake = Arc::new(Fake {
        now: Mutex::new(now),
        sent: Mutex::default(),
        asked: Mutex::default(),
        refuse: Mutex::default(),
    });
    let host = Arc::clone(&fake);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let host = Arc::clone(&host);
            std::thread::spawn(move || {
                let _: Request = receive(&mut stream).unwrap();
                let hello = Reply::Hello {
                    major: MAJOR,
                    instance: "fake".into(),
                    build: Default::default(),
                };
                send(&mut stream, CONTROL, &hello).unwrap();
                let request: Request = receive(&mut stream).unwrap();
                *host.asked.lock().unwrap() += 1;
                std::thread::sleep(slow);
                let w = host.now.lock().unwrap().clone();
                let reply = match (request, brief) {
                    _ if *host.refuse.lock().unwrap() => Reply::Error("工作区读不出来".into()),
                    (Request::BriefAll(since, _), true) => {
                        let mut sent = host.sent.lock().unwrap();
                        let base = sent.iter().find(|i| Some(i.stamp) == since);
                        let (delta, index) = diff(w, base, true).unwrap();
                        sent.push(index);
                        Reply::WorkspaceDelta(Box::new(delta))
                    }
                    (Request::Workspace, false) => Reply::Workspace(w),
                    (other, _) => panic!("not a refresh: {other:?}"),
                };
                send(&mut stream, CONTROL, &reply).unwrap();
            });
        }
    });
    (socket, fake)
}
fn understood(brief: bool) -> Understood {
    Understood::of(&RecognitionStatus {
        brief,
        brief_all: brief,
        ..RecognitionStatus::default()
    })
}
fn revision(reply: std::io::Result<Reply>) -> u64 {
    match reply {
        Ok(Reply::Workspace(w)) => w.revision,
        other => panic!("{other:?}"),
    }
}
type Asked = std::io::Result<Reply>;
/// The window and the menu bar asking for one revision at once, as they do on each status.
fn both_ask(socket: &std::path::Path, brief: bool, at: u64) -> (Asked, Asked) {
    let (window, panel) = (Feed::of(socket), Feed::of(socket));
    let first = std::thread::spawn(move || window.workspace(understood(brief), Some(at)));
    std::thread::sleep(Duration::from_millis(60));
    let second = panel.workspace(understood(brief), Some(at));
    (first.join().unwrap(), second)
}
fn both(socket: &std::path::Path, brief: bool, at: u64) -> (u64, u64) {
    let (first, second) = both_ask(socket, brief, at);
    (revision(first), revision(second))
}

/// With 5088 sessions the host changed about once a second and was asked for each change
/// three times (the window twice, the menu bar once), ~40 ms of CPU each: most of what it
/// did (11-19). The window and the menu bar now get one revision with one fetch.
#[test]
fn the_window_and_the_menu_bar_fetch_each_revision_once() {
    let (socket, fake) = serve("feed-once", true, Duration::from_millis(300));
    // The window's store and the menu bar each keep the feed for the app's life.
    let _kept = Feed::of(&socket);
    assert_eq!(both(&socket, true, 7), (7, 7));
    assert_eq!(
        *fake.asked.lock().unwrap(),
        1,
        "the second asker fetched again"
    );
    // A status that queued behind the refresh, and a refresh after one that just landed.
    assert_eq!(
        revision(Feed::of(&socket).workspace(understood(true), Some(7))),
        7
    );
    assert_eq!(
        revision(Feed::of(&socket).workspace(understood(true), Some(3))),
        7
    );
    assert_eq!(
        *fake.asked.lock().unwrap(),
        1,
        "a held revision was fetched again"
    );
    // The next revision is the host's to send.
    fake.now.lock().unwrap().revision = 8;
    assert_eq!(both(&socket, true, 8), (8, 8));
    assert_eq!(*fake.asked.lock().unwrap(), 2);
}

/// After a dropped link the host may be a new one: what is held does not answer then.
#[test]
fn a_refresh_after_a_dropped_link_asks_the_host() {
    let (socket, fake) = serve("feed-relink", true, Duration::ZERO);
    let feed = Feed::of(&socket);
    assert_eq!(revision(feed.workspace(understood(true), Some(7))), 7);
    assert_eq!(revision(feed.workspace(understood(true), None)), 7);
    assert_eq!(*fake.asked.lock().unwrap(), 2);
}

/// A host from before `Brief` sends whole workspaces, which nothing holds: each asker
/// still gets one, after the fetch in flight rather than beside it.
#[test]
fn a_host_without_brief_still_answers_everyone() {
    let (socket, fake) = serve("feed-old", false, Duration::from_millis(200));
    assert_eq!(both(&socket, false, 7), (7, 7));
    assert_eq!(*fake.asked.lock().unwrap(), 2);
}

/// A refresh the host refused is the answer for whoever waited on it too; the next
/// status asks again.
#[test]
fn a_failed_refresh_is_the_answer_for_whoever_waited_on_it() {
    let (socket, fake) = serve("feed-refused", true, Duration::from_millis(300));
    *fake.refuse.lock().unwrap() = true;
    let (first, second) = both_ask(&socket, true, 7);
    let words = |r: Asked| r.unwrap_err().to_string();
    assert_eq!(words(first), "工作区读不出来");
    assert_eq!(words(second), "工作区读不出来");
    assert_eq!(*fake.asked.lock().unwrap(), 1);
    *fake.refuse.lock().unwrap() = false;
    assert_eq!(
        revision(Feed::of(&socket).workspace(understood(true), Some(7))),
        7
    );
    assert_eq!(*fake.asked.lock().unwrap(), 2);
}
