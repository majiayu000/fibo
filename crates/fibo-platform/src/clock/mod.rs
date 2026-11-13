//! Local calendar time. std has no way to read it, and how the C runtime offers it differs.

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod imp;

/// One moment as this machine's clock and calendar read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    /// 0 is Sunday.
    pub weekday: i32,
    /// 1 is January.
    pub month: i32,
    pub day: i32,
    /// Seconds local time is ahead of UTC at that moment.
    pub utc_offset: i64,
}

/// A Unix timestamp in local time.
pub fn local(at: u64) -> LocalTime {
    let (tm, utc_offset) = imp::local(at);
    LocalTime {
        year: tm.tm_year + 1900,
        hour: tm.tm_hour,
        minute: tm.tm_min,
        second: tm.tm_sec,
        weekday: tm.tm_wday,
        month: tm.tm_mon + 1,
        day: tm.tm_mday,
        utc_offset,
    }
}

/// Seconds `local` is ahead of `utc`, two readings of one moment. They are at most a day
/// apart, so a different year means exactly one day and says which way.
#[cfg(any(windows, test))]
fn offset_from_utc(local: &libc::tm, utc: &libc::tm) -> i64 {
    const DAY: i64 = 86_400;
    let days = match local.tm_year - utc.tm_year {
        0 => local.tm_yday - utc.tm_yday,
        year => year.signum(),
    };
    i64::from(days) * DAY
        + i64::from(local.tm_hour - utc.tm_hour) * 3600
        + i64::from(local.tm_min - utc.tm_min) * 60
        + i64::from(local.tm_sec - utc.tm_sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two readings of one moment, set by hand: each zone on either side of a day and a
    /// year boundary, since Windows works its offset out this way.
    #[test]
    fn the_offset_comes_out_of_two_readings() {
        let tm = |year, yday, hour, min| {
            // SAFETY: all-zero is a valid `tm`, plain integers (and a null zone on macOS).
            let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
            (tm.tm_year, tm.tm_yday, tm.tm_hour, tm.tm_min) = (year, yday, hour, min);
            tm
        };
        // Same day.
        assert_eq!(
            offset_from_utc(&tm(126, 10, 13, 0), &tm(126, 10, 5, 0)),
            8 * 3600
        );
        // Tokyo is already tomorrow.
        assert_eq!(
            offset_from_utc(&tm(126, 11, 1, 0), &tm(126, 10, 16, 0)),
            9 * 3600
        );
        // New York is still yesterday.
        assert_eq!(
            offset_from_utc(&tm(126, 10, 20, 0), &tm(126, 11, 1, 0)),
            -5 * 3600
        );
        // Across new year, both ways.
        assert_eq!(
            offset_from_utc(&tm(127, 0, 1, 0), &tm(126, 364, 16, 0)),
            9 * 3600
        );
        assert_eq!(
            offset_from_utc(&tm(126, 364, 20, 0), &tm(127, 0, 1, 0)),
            -5 * 3600
        );
        // A zone with minutes: India.
        assert_eq!(
            offset_from_utc(&tm(126, 10, 5, 30), &tm(126, 10, 0, 0)),
            19_800
        );
    }

    /// On a system that reports the offset itself, the worked-out one must agree with it,
    /// here and over a year of moments including the zone's clock changes.
    #[cfg(unix)]
    #[test]
    fn the_worked_out_offset_agrees_with_the_system() {
        for t in (1_767_225_600..1_798_761_600).step_by(6 * 3600 + 17) {
            let (tm, offset) = imp::local(t);
            // SAFETY: gmtime_r writes only into the local handed to it.
            let utc = unsafe {
                let at = t as libc::time_t;
                let mut utc = std::mem::zeroed::<libc::tm>();
                libc::gmtime_r(&at, &mut utc);
                utc
            };
            assert_eq!(offset_from_utc(&tm, &utc), offset, "at {t}");
        }
    }

    /// Fields are read by hand since Windows' `libc` has no `strftime`: they must say what
    /// that said.
    #[cfg(unix)]
    #[test]
    fn the_fields_read_as_strftime_does() {
        for t in (1_767_225_600..1_798_761_600).step_by(6 * 3600 + 17) {
            let mut buf = [0u8; 16];
            let (tm, _) = imp::local(t);
            // SAFETY: strftime writes only into the buffer handed to it, within its length.
            let len = unsafe {
                libc::strftime(
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                    c"%H:%M:%S".as_ptr(),
                    &tm,
                )
            };
            let at = local(t);
            assert_eq!(
                format!("{:02}:{:02}:{:02}", at.hour, at.minute, at.second),
                String::from_utf8_lossy(&buf[..len]),
                "at {t}"
            );
        }
    }
}
