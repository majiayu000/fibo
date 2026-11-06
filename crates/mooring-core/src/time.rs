//! Time labels: how long ago something happened, in the words every surface shows (the
//! window, the menu bar, `mooring` output). The only place these words are formed.
use std::time::{SystemTime, UNIX_EPOCH};

const DAY: u64 = 86_400;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
/// Local wall-clock time of a Unix timestamp, as `13:04:00`.
pub fn clock(at: u64) -> String {
    let (tm, _) = local(at);
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}
/// The local calendar fields of a Unix timestamp, and how far local time is ahead of UTC then.
#[cfg(unix)]
fn local(t: u64) -> (libc::tm, i64) {
    // SAFETY: localtime_r writes only into the local handed to it.
    let tm = unsafe {
        let t = t as libc::time_t;
        let mut tm = std::mem::zeroed::<libc::tm>();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    (tm, tm.tm_gmtoff)
}
/// The C runtime's `tm` has no offset field on Windows, so the same moment is read both ways.
/// The runtime goes by `TZ` when that is set, and otherwise by the system's zone.
#[cfg(windows)]
fn local(t: u64) -> (libc::tm, i64) {
    // SAFETY: both calls write only into the locals handed to them.
    unsafe {
        let t = t as libc::time_t;
        let mut tm = std::mem::zeroed::<libc::tm>();
        let mut utc = std::mem::zeroed::<libc::tm>();
        libc::localtime_s(&mut tm, &t);
        libc::gmtime_s(&mut utc, &t);
        (tm, offset_from_utc(&tm, &utc))
    }
}
/// Seconds `local` is ahead of `utc`, two readings of one moment. They are at most a day
/// apart, so a different year means exactly one day and says which way.
#[cfg(any(windows, test))]
fn offset_from_utc(local: &libc::tm, utc: &libc::tm) -> i64 {
    let days = match local.tm_year - utc.tm_year {
        0 => local.tm_yday - utc.tm_yday,
        year => year.signum(),
    };
    i64::from(days) * DAY as i64
        + i64::from(local.tm_hour - utc.tm_hour) * 3600
        + i64::from(local.tm_min - utc.tm_min) * 60
        + i64::from(local.tm_sec - utc.tm_sec)
}
/// When a future moment falls, as a person would say it: `今天 22:37`, `明天 09:00`,
/// `周五 07:15` within the week, `11月8日` beyond it.
pub fn upcoming(at: u64, now: u64) -> String {
    let ((then, then_offset), (_, now_offset)) = (local(at), local(now));
    let hm = format!("{:02}:{:02}", then.tm_hour, then.tm_min);
    // Day numbers since the epoch in local time, so midnight is the boundary.
    let day = |t: u64, offset: i64| (t as i64 + offset) / DAY as i64;
    match day(at, then_offset) - day(now, now_offset) {
        d if d <= 0 => format!("今天 {hm}"),
        1 => format!("明天 {hm}"),
        d if d < 7 => {
            let names = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
            format!("{} {hm}", names[then.tm_wday.clamp(0, 6) as usize])
        }
        _ => format!("{}月{}日", then.tm_mon + 1, then.tm_mday),
    }
}
pub fn age(at: u64) -> String {
    let minutes = now().saturating_sub(at) / 60;
    if minutes == 0 {
        "刚刚".into()
    } else if minutes < 60 {
        format!("{minutes} 分钟")
    } else if minutes < 1440 {
        format!("{} 小时", minutes / 60)
    } else {
        format!("{} 天", minutes / 1440)
    }
}
/// How long ago, in a row's few columns and on a pane's state: `4分`, `2时`, `1天`, or
/// 刚刚, as the v3 design writes it everywhere.
pub fn short_ago(updated: u64, now: u64) -> String {
    match now.saturating_sub(updated) {
        age if age < 60 => "刚刚".into(),
        age if age < 3600 => format!("{}分", age / 60),
        age if age < DAY => format!("{}时", age / 3600),
        age => format!("{}天", age / DAY),
    }
}
/// A stretch of time, as `86 分钟` or `3 小时 5 分`.
pub fn span_label(secs: u64) -> String {
    match secs / 60 {
        0 => format!("{secs} 秒"),
        m if m < 120 => format!("{m} 分钟"),
        m if m < 2 * 1440 => format!("{} 小时 {} 分", m / 60, m % 60),
        m => format!("{} 天", m / 1440),
    }
}
pub fn recency(updated: u64, now: u64) -> &'static str {
    match now.saturating_sub(updated) {
        age if age < DAY => "最近 24 小时",
        age if age < 7 * DAY => "最近 7 天",
        _ => "更早",
    }
}
pub fn ago(updated: u64, now: u64) -> String {
    match now.saturating_sub(updated) {
        age if age < 60 => "刚刚".into(),
        age if age < 3600 => format!("{} 分钟前", age / 60),
        age if age < DAY => format!("{} 小时前", age / 3600),
        age if age < 30 * DAY => format!("{} 天前", age / DAY),
        _ => "超过 30 天".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_time_labels() {
        assert_eq!(ago(1000, 1030), "刚刚");
        assert_eq!(ago(0, 720), "12 分钟前");
        assert_eq!(ago(0, 3 * 3600), "3 小时前");
        assert_eq!(short_ago(1000, 1030), "刚刚");
        assert_eq!(short_ago(0, 720), "12分");
        assert_eq!(short_ago(0, 3 * 3600 + 59), "3时");
        assert_eq!(short_ago(0, 40 * 86_400), "40天");
        assert_eq!(recency(0, 3600), "最近 24 小时");
        assert_eq!(span_label(5160), "86 分钟");
        assert_eq!(span_label(3 * 3600 + 300), "3 小时 5 分");
        assert_eq!(span_label(30), "30 秒");
        assert_eq!(span_label(5 * 86_400), "5 天");
        assert_eq!(recency(0, 3 * 86_400), "最近 7 天");
        assert_eq!(recency(0, 30 * 86_400), "更早");
    }
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
            let (tm, offset) = local(t);
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
    /// Written by hand since Windows' `libc` has no `strftime`: it must say what that said.
    #[cfg(unix)]
    #[test]
    fn the_clock_reads_as_strftime_did() {
        for t in (1_767_225_600..1_798_761_600).step_by(6 * 3600 + 17) {
            let mut buf = [0u8; 16];
            let (tm, _) = local(t);
            // SAFETY: strftime writes only into the buffer handed to it, within its length.
            let len = unsafe {
                libc::strftime(
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                    c"%H:%M:%S".as_ptr(),
                    &tm,
                )
            };
            assert_eq!(clock(t), String::from_utf8_lossy(&buf[..len]), "at {t}");
        }
    }
    #[test]
    fn upcoming_labels_count_local_days() {
        let now = 1_793_000_000;
        assert!(upcoming(now + 60, now).ends_with(&clock(now + 60)[..5]));
        assert!(upcoming(now + 2 * DAY, now).starts_with('周'));
        assert!(upcoming(now + 30 * DAY, now).ends_with('日'));
    }
}
