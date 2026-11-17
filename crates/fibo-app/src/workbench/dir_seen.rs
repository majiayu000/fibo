//! Whether a space's folder is there, as rendering asks it: the sidebar redraws every
//! second while something runs, and from the App a `stat` of a folder under a protected
//! place (Desktop, Documents) costs far more than it does from a shell. With 33 spaces
//! that was a tenth of the main thread on 11-17; asked again after `FRESH` instead.
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long an answer stands. A folder deleted meanwhile is marked gone this much later.
const FRESH: Duration = Duration::from_secs(2);

thread_local! {
    static SEEN: RefCell<FxHashMap<String, (Instant, bool)>> = RefCell::default();
}

pub(super) fn is_dir(path: &str) -> bool {
    SEEN.with_borrow_mut(|seen| {
        let now = Instant::now();
        if let Some((at, there)) = seen.get(path)
            && now.duration_since(*at) < FRESH
        {
            return *there;
        }
        let there = Path::new(path).is_dir();
        seen.insert(path.to_owned(), (now, there));
        there
    })
}
