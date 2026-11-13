/// The C runtime's `tm` has no offset field on Windows, so the same moment is read both ways.
/// The runtime goes by `TZ` when that is set, and otherwise by the system's zone.
pub fn local(t: u64) -> (libc::tm, i64) {
    // SAFETY: both calls write only into the locals handed to them.
    unsafe {
        let t = t as libc::time_t;
        let mut tm = std::mem::zeroed::<libc::tm>();
        let mut utc = std::mem::zeroed::<libc::tm>();
        libc::localtime_s(&mut tm, &t);
        libc::gmtime_s(&mut utc, &t);
        (tm, super::offset_from_utc(&tm, &utc))
    }
}
