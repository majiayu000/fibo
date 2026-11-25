use super::*;
use fibo_core::protocol::{receive, send, CONTROL, MAJOR};
use fibo_core::workspace_delta::{diff, Index};
use fibo_platform::local;
use std::sync::mpsc;

/// What a request asked for, as the fake host saw it.
#[derive(Debug, PartialEq)]
enum Asked {
    Status,
    Whole,
    Brief(Option<u64>),
}

/// Serves `workspace` (which the test may change between reads) as a host that does or
/// does not understand `BriefAll`, one request per connection as `rpc` sends them.
fn host(
    socket: &Path,
    workspace: Arc<Mutex<Workspace>>,
    brief_all: bool,
) -> (mpsc::Receiver<(Asked, usize)>, std::thread::JoinHandle<()>) {
    let listener = local::Listener::bind(socket).unwrap();
    let (tell, asked) = mpsc::channel();
    let serving = std::thread::spawn(move || {
        let mut sent: Vec<Index> = Vec::new();
        while let Ok(mut stream) = listener.accept() {
            let Ok(Request::Hello { .. }) = receive::<Request>(&mut stream) else {
                return;
            };
            let hello = Reply::Hello {
                major: MAJOR,
                instance: "fake".into(),
                build: Default::default(),
            };
            send(&mut stream, CONTROL, &hello).unwrap();
            let w = workspace.lock().unwrap().clone();
            let (asked, reply) = match receive::<Request>(&mut stream).unwrap() {
                Request::RecognitionStatus => {
                    let status = RecognitionStatus {
                        brief: brief_all,
                        brief_all,
                        ..Default::default()
                    };
                    (Asked::Status, Reply::Recognition(status))
                }
                Request::Workspace => (Asked::Whole, Reply::Workspace(w)),
                Request::BriefAll(since, _) if brief_all => {
                    let base = since.and_then(|s| sent.iter().find(|i| i.stamp == s));
                    let (delta, index) = diff(w, base, true).unwrap();
                    sent.push(index);
                    (Asked::Brief(since), Reply::WorkspaceDelta(Box::new(delta)))
                }
                // A host before BriefAll drops the connection.
                Request::BriefAll(since, _) => {
                    tell.send((Asked::Brief(since), 0)).unwrap();
                    continue;
                }
                other => panic!("unexpected {other:?}"),
            };
            let bytes = serde_json::to_vec(&reply).unwrap().len();
            tell.send((asked, bytes)).unwrap();
            send(&mut stream, CONTROL, &reply).unwrap();
        }
    });
    (asked, serving)
}

fn same(a: &Workspace, b: &Workspace) -> bool {
    serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap()
}

fn socket() -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/tmp/fibo-held-{}", uuid::Uuid::new_v4().simple()))
}

/// #527: a phone refresh asked the host for the whole workspace every time. After the
/// first read, a refresh asks for what changed since the one it holds, gets far less,
/// and still ends with the whole workspace as the host has it; a host before `BriefAll`
/// is never sent one and serves the whole workspace as before. (One test: what this
/// process holds is shared.)
#[test]
fn a_refresh_after_the_first_asks_only_for_what_changed() {
    HELD.lock().unwrap().clear();
    UNDERSTOOD.store(UNKNOWN, Ordering::Relaxed);
    let path = socket();
    let shared = Arc::new(Mutex::new(fibo_core::sample::every_state()));
    let (asked, serving) = host(&path, Arc::clone(&shared), true);

    let first = workspace(&path).unwrap();
    assert!(same(&first, &shared.lock().unwrap()));
    assert_eq!(asked.recv().unwrap().0, Asked::Status);
    let (Asked::Brief(None), whole) = asked.recv().unwrap() else {
        panic!("the first read asks for everything as a brief");
    };
    {
        let mut w = shared.lock().unwrap();
        let id = w.agent_sessions[0].id.clone();
        Arc::make_mut(w.content.get_mut(&id).unwrap()).title = "改过的标题".into();
    }
    let second = workspace(&path).unwrap();
    assert!(same(&second, &shared.lock().unwrap()), "put back whole");
    let (Asked::Brief(Some(_)), changed) = asked.recv().unwrap() else {
        panic!("a refresh asks for what changed since the one held");
    };
    assert!(
        changed * 4 < whole,
        "only the change goes: {changed} of {whole} bytes"
    );
    drop(serving);

    // A host before BriefAll, after an update swapped it in.
    UNDERSTOOD.store(UNKNOWN, Ordering::Relaxed);
    let old = socket();
    let (asked, _serving) = host(&old, Arc::clone(&shared), false);
    let w = workspace(&old).unwrap();
    assert!(same(&w, &shared.lock().unwrap()));
    assert_eq!(asked.recv().unwrap().0, Asked::Status);
    assert_eq!(asked.recv().unwrap().0, Asked::Whole);
    workspace(&old).unwrap();
    assert_eq!(asked.recv().unwrap().0, Asked::Whole, "never sent a brief");
    for p in [path, old] {
        let _ = std::fs::remove_file(p);
    }
}
