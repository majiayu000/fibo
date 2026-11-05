use crate::host::store::Store;
use mooring_core::protocol::Request;

#[test]
fn a_launch_during_another_action_starts_once_it_ends() {
    let (tx, _rx) = futures::channel::mpsc::unbounded();
    let mut store = Store::new("/nonexistent/mooring.sock".into(), tx);
    let task = Some("task".to_owned());
    assert!(store.admit_launch(true, &task, &None));
    assert_eq!(store.next_launch(), None);

    store.begin_busy(None);
    assert!(!store.admit_launch(false, &None, &None));
    assert!(!store.admit_launch(true, &task, &None));
    store.hold(Request::Workspace);
    store.end_busy();
    // Requests asked for meanwhile go first; then the latest launch, once.
    assert!(matches!(store.next_queued(), Some(Request::Workspace)));
    assert_eq!(store.next_launch(), Some((true, task, None)));
    assert_eq!(store.next_launch(), None);
}
