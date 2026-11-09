use super::store::Store;
use crate::connection::StatusLink;
use mooring_core::protocol::{RecognitionPhase, RecognitionStatus};

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
