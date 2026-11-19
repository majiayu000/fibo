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
        let content = host
            .workspace
            .content
            .get_mut(&session)
            .map(std::sync::Arc::make_mut)
            .unwrap();
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

/// A refresh is worked out on the host's own content, copying only what overlays change
/// and what goes out. It must be the delta of the snapshot every other reply sends: with an
/// overlay on a current session, an old session no one is sent, and content changed since.
#[test]
fn a_refresh_on_the_hosts_own_content_is_the_delta_of_its_snapshot() {
    use fibo_core::workspace_delta::{diff, Hashes};
    let (mut host, _root) = super::tests::fixture();
    host.workspace = fibo_core::sample::every_state();
    let live = (host.workspace.agent_sessions.iter())
        .find(|s| s.current && s.terminal.is_some() && host.workspace.content.contains_key(&s.id))
        .expect("a sample session in a terminal")
        .clone();
    let c = host
        .workspace
        .content
        .get_mut(&live.id)
        .map(std::sync::Arc::make_mut)
        .unwrap();
    c.turn = fibo_core::content::Turn::Completed;
    let content = c.clone();
    host.title_busy
        .insert(live.terminal.clone().unwrap(), (live.id.clone(), 7));
    let mut old = live.clone();
    (old.id, old.terminal, old.current, old.updated) = ("old".into(), None, false, 0);
    let mut past = old.clone();
    (past.id, past.updated) = ("past".into(), fibo_core::content::now());
    host.workspace.agent_sessions.extend([old, past]);
    host.workspace
        .content
        .insert("old".into(), std::sync::Arc::new(content.clone()));
    host.workspace
        .content
        .insert("past".into(), std::sync::Arc::new(content));
    let same =
        |a: &Delta, b: &Delta| serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap();

    let mut hashes = Hashes::default();
    let (fresh, fresh_index) = diff(host.snapshot(), None, true).unwrap();
    let (lent, index) = host.brief(None, true, &mut hashes).unwrap();
    assert!(same(&fresh, &lent));
    assert_eq!(fresh_index.stamp, index.stamp);
    assert!(!lent.content.contains_key("old") && !lent.sessions.iter().any(|s| s.id == "old"));
    assert_eq!(
        lent.content[&live.id].signal, "标题",
        "the overlay is in the reply"
    );
    assert_eq!(
        host.workspace.content[&live.id].turn,
        fibo_core::content::Turn::Completed,
        "and not in the host's own content"
    );

    let other = String::from("past");
    host.workspace
        .content
        .get_mut(&other)
        .map(std::sync::Arc::make_mut)
        .unwrap()
        .title = "改了".into();
    let (fresh, fresh_next) = diff(host.snapshot(), Some(&fresh_index), true).unwrap();
    let (lent, next) = host.brief(Some(&index), true, &mut hashes).unwrap();
    assert!(same(&fresh, &lent));
    assert_eq!(fresh_next.stamp, next.stamp);
    assert_eq!(lent.content.keys().collect::<Vec<_>>(), [&other]);
}

/// The snapshot every reply starts from shares the host's content: what no overlay changed
/// is the host's own allocation, compared by pointer and never copied.
#[test]
fn a_snapshot_shares_the_content_no_overlay_changed() {
    let (mut host, _root) = super::tests::fixture();
    host.workspace = fibo_core::sample::every_state();
    let w = host.snapshot();
    assert!(!w.content.is_empty());
    for (id, c) in &w.content {
        assert!(
            std::sync::Arc::ptr_eq(c, &host.workspace.content[id]),
            "{id} was copied"
        );
    }
}
