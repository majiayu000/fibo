//! Another computer's window choosing a space's folder here: it lists this machine's
//! folders and makes a new one over the same link its terminals use (`ListDirs`, `MakeDir`).
#[path = "remote_v2/common.rs"]
mod common;
use common::*;
use fibo_core::protocol::{rpc, Reply, Request};

#[test]
fn another_computers_window_lists_folders_here_and_makes_one() {
    let h = Host::start();
    let base = scratch(&h, "code");
    std::fs::create_dir_all(base.join("site")).unwrap();
    let base = base.to_string_lossy().into_owned();
    let list = |h: &Host| match rpc(&h.socket(), &Request::ListDirs { path: base.clone() }) {
        Ok(Reply::Dirs(dirs)) => dirs,
        other => panic!("{other:?}"),
    };
    assert_eq!(list(&h), [format!("{base}/site")]);
    let path = format!("{base}/new one");
    let made = rpc(&h.socket(), &Request::MakeDir { path: path.clone() }).unwrap();
    assert!(matches!(&made, Reply::Text(p) if *p == path), "{made:?}");
    assert_eq!(
        list(&h),
        [format!("{base}/new one"), format!("{base}/site")]
    );
    let gone = format!("{base}/nope/deeper");
    assert!(rpc(&h.socket(), &Request::MakeDir { path: gone }).is_err());
}
