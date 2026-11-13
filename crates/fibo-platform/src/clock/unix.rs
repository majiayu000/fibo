/// The local calendar fields of a Unix timestamp, and how far local time is ahead of UTC then.
pub fn local(t: u64) -> (libc::tm, i64) {
    // SAFETY: localtime_r writes only into the local handed to it.
    let tm = unsafe {
        let t = t as libc::time_t;
        let mut tm = std::mem::zeroed::<libc::tm>();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    (tm, tm.tm_gmtoff)
}
