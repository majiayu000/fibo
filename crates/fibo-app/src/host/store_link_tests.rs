use super::store::Store;
use crate::connection::StatusLink;
use fibo_core::protocol::{RecognitionPhase, RecognitionStatus};

/// Woken from sleep, the host found a deleted log folder and said recognition was
/// Interrupted; every connection still worked, yet the window showed 「后台连接断开」 for
/// good. Only a dropped link is a cut; a status of any phase means the host answered.
#[test]
fn a_host_saying_interrupted_is_not_a_cut_link() {
    let (tx, _rx) = futures::channel::mpsc::unbounded();
    let mut store = Store::new("/nonexistent/mooring.sock".into(), tx);
    let interrupted = RecognitionStatus {
        phase: RecognitionPhase::Interrupted,
        detail: "日志目录暂不可用，等待重建事件".into(),
        ..RecognitionStatus::default()
    };
    store.take_link(StatusLink::Status(Box::new(interrupted)));
    assert!(!store.link_down(), "a host status is not a cut link");
    assert_eq!(store.recognition().phase, RecognitionPhase::Interrupted);

    store.take_link(StatusLink::Lost("识别状态连接中断：broken pipe".into()));
    assert!(store.link_down());
    assert_eq!(
        store.cut_off_reason(),
        Some("识别状态连接中断：broken pipe")
    );
    // The last status stays on screen while the link is down.
    assert_eq!(store.recognition().phase, RecognitionPhase::Interrupted);

    store.take_link(StatusLink::Status(Box::default()));
    assert!(!store.link_down());
}

/// One background refresh cut off mid-reply (a newer host taking over) left 「RPC receive
/// reply: failed to fill whole buffer」 on the window for good, though every refresh after
/// it worked. A success clears what a refresh put there, and only that.
#[test]
fn a_refresh_that_works_again_clears_only_its_own_error() {
    let (tx, _rx) = futures::channel::mpsc::unbounded();
    let mut store = Store::new("/nonexistent/mooring.sock".into(), tx);
    let mut shown = None;
    let cut = "RPC receive reply: failed to fill whole buffer".to_owned();
    store.settle_sync(&mut shown, Some(cut.clone()));
    assert_eq!(shown.as_deref(), Some(cut.as_str()));
    store.settle_sync(&mut shown, None);
    assert_eq!(
        shown, None,
        "a refresh that worked left the last one's error"
    );

    // A user action's error stays through a refresh, failed before or not.
    store.settle_sync(&mut shown, Some(cut));
    shown = Some("没发出去：后台没有响应".into());
    store.settle_sync(&mut shown, None);
    assert_eq!(shown.as_deref(), Some("没发出去：后台没有响应"));
}

/// The host sends several statuses per change (titles, hints, commands); each one that
/// came during a refresh queued another fetch of the same revision, twice per change
/// (11-19). The refresh in flight looks at the revision once it lands instead.
#[test]
fn statuses_during_a_refresh_do_not_queue_another() {
    let (tx, _rx) = futures::channel::mpsc::unbounded();
    let mut store = Store::new("/nonexistent/mooring.sock".into(), tx);
    let at = |revision| RecognitionStatus {
        workspace_revision: revision,
        ..RecognitionStatus::default()
    };
    store.set_recognition(at(8));
    assert!(store.follow(8, 7));
    assert!(store.begin_sync());
    store.set_recognition(at(8));
    assert!(
        !store.follow(8, 7),
        "a status during the refresh fetched again"
    );
    assert!(!store.end_sync());
    // It landed at 8: nothing more. Had the host moved on, it is behind.
    assert!(!store.behind(8));
    store.set_recognition(at(9));
    assert!(store.behind(8) && store.follow(9, 8));
}
