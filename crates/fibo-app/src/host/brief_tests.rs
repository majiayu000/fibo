use super::{Held, Understood};
use fibo_core::delivery::Settled;
use fibo_core::protocol::{CONTROL, MAJOR, Reply, Request, receive, send};
use fibo_core::workspace_delta::{Index, diff};
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};

/// A host that answers every `Brief` with `now` against what it was asked for; `stale`
/// answers one against the last it sent instead, as a reply that crossed another does.
struct Fake {
    now: Mutex<fibo_core::model::Workspace>,
    sent: Mutex<Vec<Index>>,
    asked: Mutex<Vec<Option<u64>>>,
    stale: Mutex<bool>,
}
fn serve(name: &str) -> (std::path::PathBuf, Arc<Fake>) {
    let dir = std::env::temp_dir().join(format!("fibo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let socket = dir.join("workbench.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let fake = Arc::new(Fake {
        now: Mutex::new(fibo_core::sample::every_state()),
        sent: Mutex::default(),
        asked: Mutex::default(),
        stale: Mutex::default(),
    });
    let host = Arc::clone(&fake);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let _: Request = receive(&mut stream).unwrap();
            let hello = Reply::Hello {
                major: MAJOR,
                instance: "fake".into(),
                build: Default::default(),
            };
            send(&mut stream, CONTROL, &hello).unwrap();
            let (since, fields) = match receive(&mut stream).unwrap() {
                Request::Brief(since, _) => (since, false),
                Request::BriefAll(since, _) => (since, true),
                other => panic!("not brief: {other:?}"),
            };
            host.asked.lock().unwrap().push(since);
            let mut sent = host.sent.lock().unwrap();
            let stale = std::mem::take(&mut *host.stale.lock().unwrap());
            let base = match stale {
                true => sent.last(),
                false => sent.iter().find(|i| Some(i.stamp) == since),
            };
            let (delta, index) = diff(host.now.lock().unwrap().clone(), base, fields).unwrap();
            sent.push(index);
            send(
                &mut stream,
                CONTROL,
                &Reply::WorkspaceDelta(Box::new(delta)),
            )
            .unwrap();
        }
    });
    (socket, fake)
}
const ALL: Understood = Understood {
    brief: true,
    all: true,
};
fn workspace(settled: Settled) -> fibo_core::model::Workspace {
    match settled {
        Settled::Confirmed(Reply::Workspace(w)) => w,
        other => panic!("{other:?}"),
    }
}

/// Over a slow link to another machine, a whole workspace on every change kept the link
/// full and typing there seconds behind. The second refresh names the first's workspace
/// and the window still gets the whole one back.
#[test]
fn a_refresh_asks_for_what_changed_since_the_last_and_gets_the_whole() {
    let (socket, fake) = serve("brief-refresh");
    let held = Held::default();
    let first = workspace(held.deliver(&socket, Request::Workspace, ALL));
    let session = first.agent_sessions[0].id.clone();
    fake.now
        .lock()
        .unwrap()
        .content
        .get_mut(&session)
        .map(std::sync::Arc::make_mut)
        .unwrap()
        .title = "新标题".into();
    let second = workspace(held.deliver(&socket, Request::Workspace, ALL));
    let asked = fake.asked.lock().unwrap().clone();
    assert_eq!(asked[0], None);
    assert_eq!(asked[1], Some(fake.sent.lock().unwrap()[0].stamp));
    assert_eq!(second.content[&session].title, "新标题");
    assert_eq!(second.agent_sessions.len(), first.agent_sessions.len());
}

/// A delta against a workspace this window no longer holds is asked for again whole.
#[test]
fn a_delta_whose_base_is_gone_here_is_asked_for_whole() {
    let (socket, fake) = serve("brief-gone");
    let held = Held::default();
    workspace(held.deliver(&socket, Request::Workspace, ALL));
    held.0.lock().unwrap().clear();
    *fake.stale.lock().unwrap() = true;
    let w = workspace(held.deliver(&socket, Request::Workspace, ALL));
    assert_eq!(fake.asked.lock().unwrap().len(), 3, "asked again");
    assert_eq!(fake.asked.lock().unwrap()[2], None);
    assert_eq!(
        w.agent_sessions.len(),
        fake.now.lock().unwrap().agent_sessions.len()
    );
}
