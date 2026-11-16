use super::*;
use fibo_core::model::{Edit, Space};
use fibo_core::workspace_delta::Delta;
use fibo_platform::local;

fn ask(window: &mut local::Stream, since: Option<u64>, request: Request) -> Delta {
    send(window, CONTROL, &Request::Brief(since, Box::new(request))).unwrap();
    take(window)
}
fn take(window: &mut local::Stream) -> Delta {
    match receive::<Reply>(window).unwrap() {
        Reply::WorkspaceDelta(delta) => *delta,
        other => panic!("{other:?}"),
    }
}

/// Another machine's window got its whole workspace, about 3 MB, on every change, over a
/// link of about 54 KB/s, and typing there waited behind it. Asked as a `Brief`, the host
/// sends the sessions that changed since the workspace named, for a refresh and an edit.
#[test]
fn a_brief_request_gets_only_the_sessions_that_changed() {
    let (mut host, _root) = super::tests::fixture();
    host.workspace = fibo_core::sample::every_state();
    assert!(
        status::recognition_status(&host).brief && status::recognition_status(&host).brief_all,
        "the window learns it may ask"
    );
    let shared = Arc::new(Mutex::new(host));
    let (server, mut window) = local::Stream::pair().unwrap();
    let serving = Arc::clone(&shared);
    thread::spawn(move || client::serve_client(server, serving, 9));
    send(&mut window, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    assert!(matches!(
        receive::<Reply>(&mut window).unwrap(),
        Reply::Hello { .. }
    ));

    let whole = ask(&mut window, None, Request::Workspace);
    assert_eq!(whole.base, None);
    assert!(!whole.sessions.is_empty());
    let session = whole.sessions[0].id.clone();
    {
        let mut host = shared.lock().unwrap();
        let content = host.workspace.content.get_mut(&session).unwrap();
        content.title = "新标题".into();
    }
    let changed = ask(&mut window, Some(whole.stamp), Request::Workspace);
    assert_eq!(changed.base, Some(whole.stamp));
    assert_eq!(changed.content.keys().collect::<Vec<_>>(), [&session]);
    assert!(changed.sessions.is_empty());

    let space = Space {
        id: "new".into(),
        name: "新空间".into(),
        cwd: "/tmp".into(),
    };
    let edit = Request::Edit {
        request: None,
        edit: Edit::AddSpace(space),
    };
    // As `BriefAll`, of the rest only what the edit changed.
    let all = Request::BriefAll(Some(changed.stamp), Box::new(edit));
    send(&mut window, CONTROL, &all).unwrap();
    let edited = take(&mut window);
    assert_eq!(edited.base, Some(changed.stamp));
    assert!(edited.fields && edited.rest["spaces"].to_string().contains("新空间"));
    assert!(
        !edited.rest.contains_key("terminals"),
        "{:?}",
        edited.rest.keys()
    );
    assert!(edited.content.is_empty());
}
