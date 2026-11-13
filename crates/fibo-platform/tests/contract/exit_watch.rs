use crate::myself;
use fibo_platform::exit_watch::ExitWatch;
use std::sync::mpsc::channel;
use std::time::Duration;

const SOON: Duration = Duration::from_secs(10);

#[test]
fn a_watched_process_is_reported_once_when_it_ends() {
    let (ended, reports) = channel();
    let watch = ExitWatch::new(move |pid| ended.send(pid).unwrap()).unwrap();
    let dir = crate::scratch("watch-ends");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .spawn()
        .unwrap();
    assert!(watch.watch(child.id()).unwrap());
    assert!(
        reports.recv_timeout(Duration::from_millis(200)).is_err(),
        "it has not ended yet"
    );
    child.kill().unwrap();
    assert_eq!(reports.recv_timeout(SOON).unwrap(), child.id());
    child.wait().unwrap();
    assert!(
        reports.recv_timeout(Duration::from_millis(200)).is_err(),
        "reported twice"
    );
}

#[test]
fn watching_an_ended_process_says_so_or_reports_it() {
    let (ended, reports) = channel();
    let watch = ExitWatch::new(move |pid| ended.send(pid).unwrap()).unwrap();
    let dir = crate::scratch("watch-ended");
    let mut child = myself("process::helper_lasts", &dir.join("unused"))
        .spawn()
        .unwrap();
    let pid = child.id();
    child.kill().unwrap();
    child.wait().unwrap();
    if watch.watch(pid).unwrap() {
        assert_eq!(reports.recv_timeout(SOON).unwrap(), pid);
    }
}

#[test]
fn one_watch_holds_several_processes() {
    let (ended, reports) = channel();
    let watch = ExitWatch::new(move |pid| ended.send(pid).unwrap()).unwrap();
    let dir = crate::scratch("watch-several");
    let mut children: Vec<_> = (0..3)
        .map(|_| {
            myself("process::helper_lasts", &dir.join("unused"))
                .spawn()
                .unwrap()
        })
        .collect();
    for child in &children {
        assert!(watch.watch(child.id()).unwrap());
    }
    let mut want: Vec<u32> = children.iter().map(|c| c.id()).collect();
    for child in &mut children {
        child.kill().unwrap();
        child.wait().unwrap();
    }
    let mut got: Vec<u32> = (0..3)
        .map(|_| reports.recv_timeout(SOON).unwrap())
        .collect();
    want.sort_unstable();
    got.sort_unstable();
    assert_eq!(got, want);
}
