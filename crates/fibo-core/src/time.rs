//! Time labels: how long ago something happened, in the words every surface shows (the
//! window, the menu bar, `fibo` output). The only place these words are formed.
use fibo_platform::clock::local;
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
    let t = local(at);
    format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second)
}
/// When a future moment falls, as a person would say it: `今天 22:37`, `明天 09:00`,
/// `周五 07:15` within the week, `11月8日` beyond it.
pub fn upcoming(at: u64, now: u64) -> String {
    let (then, now_offset) = (local(at), local(now).utc_offset);
    let hm = format!("{:02}:{:02}", then.hour, then.minute);
    // Day numbers since the epoch in local time, so midnight is the boundary.
    let day = |t: u64, offset: i64| (t as i64 + offset) / DAY as i64;
    match day(at, then.utc_offset) - day(now, now_offset) {
        d if d <= 0 => crate::t!("今天 {hm}", hm),
        1 => crate::t!("明天 {hm}", hm),
        d if d < 7 => {
            let names = [
                crate::t!("周日"),
                crate::t!("周一"),
                crate::t!("周二"),
                crate::t!("周三"),
                crate::t!("周四"),
                crate::t!("周五"),
                crate::t!("周六"),
            ];
            format!("{} {hm}", names[then.weekday.clamp(0, 6) as usize])
        }
        _ => crate::t!("{month}月{day}日", month = then.month, day = then.day),
    }
}
pub fn age(at: u64) -> String {
    let minutes = now().saturating_sub(at) / 60;
    if minutes == 0 {
        crate::t!("刚刚").into()
    } else if minutes < 60 {
        crate::t!("{minutes} 分钟", minutes)
    } else if minutes < 1440 {
        crate::t!("{h} 小时", h = minutes / 60)
    } else {
        crate::t!("{d} 天", d = minutes / 1440)
    }
}
/// How long ago, in a row's few columns and on a pane's state: `4分`, `2时`, `1天`, or
/// 刚刚, as the v3 design writes it everywhere.
pub fn short_ago(updated: u64, now: u64) -> String {
    match now.saturating_sub(updated) {
        age if age < 60 => crate::t!("刚刚").into(),
        age if age < 3600 => crate::t!("{m}分", m = age / 60),
        age if age < DAY => crate::t!("{h}时", h = age / 3600),
        age => crate::t!("{d}天", d = age / DAY),
    }
}
/// A stretch of time, as `86 分钟` or `3 小时 5 分`.
pub fn span_label(secs: u64) -> String {
    match secs / 60 {
        0 => crate::t!("{secs} 秒", secs),
        m if m < 120 => crate::t!("{minutes} 分钟", minutes = m),
        m if m < 2 * 1440 => crate::t!("{h} 小时 {m} 分", h = m / 60, m = m % 60),
        m => crate::t!("{d} 天", d = m / 1440),
    }
}
pub fn recency(updated: u64, now: u64) -> &'static str {
    match now.saturating_sub(updated) {
        age if age < DAY => crate::t!("最近 24 小时"),
        age if age < 7 * DAY => crate::t!("最近 7 天"),
        _ => crate::t!("更早"),
    }
}
pub fn ago(updated: u64, now: u64) -> String {
    match now.saturating_sub(updated) {
        age if age < 60 => crate::t!("刚刚").into(),
        age if age < 3600 => crate::t!("{m} 分钟前", m = age / 60),
        age if age < DAY => crate::t!("{h} 小时前", h = age / 3600),
        age if age < 30 * DAY => crate::t!("{d} 天前", d = age / DAY),
        _ => crate::t!("超过 30 天").into(),
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
    #[test]
    fn upcoming_labels_count_local_days() {
        let now = 1_793_000_000;
        assert!(upcoming(now + 60, now).ends_with(&clock(now + 60)[..5]));
        assert!(upcoming(now + 2 * DAY, now).starts_with('周'));
        assert!(upcoming(now + 30 * DAY, now).ends_with('日'));
    }
}
