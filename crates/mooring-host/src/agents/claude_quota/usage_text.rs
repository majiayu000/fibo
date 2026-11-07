//! Reads what `claude -p /usage --output-format json` said. Words meant for a person, not an
//! interface, so whatever it can't tell is an error: that is not knowing, and the last reading
//! stays. "No limits" is only what it says when it prints nothing but the session's cost (an API
//! key, or no sign-in).
use jiff::{civil, tz::TimeZone, Timestamp, ToSpan};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

const SESSION: i64 = 5 * 3600;
const WEEK: i64 = 7 * 86_400;
const SLACK: i64 = 86_400;
/// From here on the account's own window, the next read comes sooner.
const NEAR: f64 = 90.;
const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

macro_rules! pattern {
    ($name:ident, $text:literal) => {
        fn $name() -> &'static Regex {
            static RE: OnceLock<Regex> = OnceLock::new();
            RE.get_or_init(|| Regex::new($text).expect("a valid pattern"))
        }
    };
}
// An HTTP status or a lost sign-in beside the figures: they may be someone else's or old.
pattern!(
    denied,
    r"(?i)\b(401|403)\b|not (logged|signed) in|signed out|sign-in (has )?expired|unauthorized|forbidden|authentication (failed|required)|invalid (access )?token"
);
// "Last 7d · 403 requests · 388 sessions" counts requests: it is no HTTP 403.
pattern!(
    counts,
    r"(?: · )?Last [0-9]+[a-z]+ · [0-9][0-9,]* requests? · [0-9][0-9,]* sessions?"
);
pattern!(
    window,
    r"^Current (session|week(?: \(([^)]+)\))?):\s*([0-9.]+)% used(?:\s*·\s*resets (.+))?$"
);
pattern!(clock, r"(?i)^(\d{1,2})(?::(\d{2}))?\s*(am|pm)$");
pattern!(
    dated,
    r"^([A-Za-z]{3})[a-z]* (\d{1,2})(?:, (\d{4}))?(?: at|,) (.+)$"
);
pattern!(colors, "\u{1b}\\[[0-9;]*m");

/// `{at, session?, week?, models, near}` as `quota.json` points at it; just `{at}` with no limits.
pub(super) fn read(text: &str, now: i64) -> Result<Value, String> {
    let text = colors().replace_all(text, "");
    let text = text.trim();
    // What follows is advice on using less, whose words are nothing to read limits from.
    let head = text.split("What's contributing").next().unwrap_or_default();
    let head = counts().replace_all(head, "");
    let lines: Vec<&str> = (head.lines().map(str::trim))
        .filter(|l| !l.is_empty())
        .collect();
    if let Some(line) = lines.iter().find(|l| denied().is_match(l)) {
        return Err((*line).to_owned());
    }
    let (mut session, mut week, mut models) = (None, None, Vec::<Value>::new());
    for line in &lines {
        let Some(m) = window().captures(line) else {
            continue;
        };
        let Ok(used) = m[3].parse::<f64>() else {
            continue;
        };
        let resets = m.get(4).map_or("", |r| r.as_str());
        if &m[1] == "session" {
            session.get_or_insert(reading(used, resets, now, SESSION));
            continue;
        }
        let scope = m.get(2).map_or("", |s| s.as_str()).trim();
        let scope = strip_only(scope);
        if scope.is_empty() || scope.eq_ignore_ascii_case("all models") {
            week.get_or_insert(reading(used, resets, now, WEEK));
        } else if !models.iter().any(|x| x["name"] == scope) {
            let mut model = reading(used, resets, now, WEEK);
            model["name"] = scope.into();
            models.push(model);
        }
    }
    if session.is_none() && week.is_none() && models.is_empty() {
        if lines.iter().any(|l| l.starts_with("Total cost:")) {
            return Ok(json!({ "at": now }));
        }
        // Still loading, the usage page refusing for a while, a format not seen yet.
        return Err(match lines.as_slice() {
            [] => "/usage 什么也没说".to_owned(),
            said => format!(
                "/usage 没给额度：{}",
                said[..said.len().min(2)]
                    .join(" ")
                    .chars()
                    .take(200)
                    .collect::<String>()
            ),
        });
    }
    // The account's own windows: when these run out it stops, so read sooner.
    let near = [&session, &week]
        .into_iter()
        .flatten()
        .any(|w| w["used"].as_f64().is_some_and(|u| u >= NEAR));
    let mut doc = json!({ "at": now, "models": models, "near": near });
    if let Some(s) = session {
        doc["session"] = s;
    }
    if let Some(w) = week {
        doc["week"] = w;
    }
    Ok(doc)
}

fn strip_only(scope: &str) -> &str {
    let cut = scope.len().saturating_sub(" only".len());
    match scope.get(cut..) {
        Some(end) if end.eq_ignore_ascii_case(" only") => &scope[..cut],
        _ => scope,
    }
}

fn reading(used: f64, resets: &str, now: i64, span: i64) -> Value {
    json!({ "used": used, "resets_at": reset_time(resets, now, span) })
}

/// "Nov 7 at 3:49pm (Asia/Shanghai)", "Oct 9, 2:59pm (UTC)", "Oct 9, 2026 at 3pm", "3pm (UTC)".
/// A window is never further ahead than its length, which settles the year or the day. 0 when
/// it reads as none of them: a window then says nothing of when, rather than the wrong time.
fn reset_time(text: &str, now: i64, span: i64) -> i64 {
    let mut s = text.trim();
    let mut zone = TimeZone::system();
    if let (Some(open), true) = (s.rfind('('), s.ends_with(')')) {
        if let Ok(named) = TimeZone::get(&s[open + 1..s.len() - 1]) {
            zone = named;
        }
        s = s[..open].trim();
    }
    let Ok(at) = Timestamp::from_second(now) else {
        return 0;
    };
    let today = at.to_zoned(zone.clone()).date();
    let moment = |date: civil::Date, (h, m): (i8, i8)| -> Option<i64> {
        let local = civil::DateTime::new(date.year(), date.month(), date.day(), h, m, 0, 0).ok()?;
        Some(local.to_zoned(zone.clone()).ok()?.timestamp().as_second())
    };
    if let Some(d) = dated().captures(s) {
        let month = d[1].to_ascii_lowercase();
        let Some(month) = MONTHS.iter().position(|m| *m == month) else {
            return 0;
        };
        let (Some(time), Ok(day)) = (hour_minute(&d[4]), d[2].parse::<i8>()) else {
            return 0;
        };
        let on = |year: i16| civil::Date::new(year, month as i8 + 1, day).ok();
        if let Some(year) = d.get(3).and_then(|y| y.as_str().parse().ok()) {
            return on(year).and_then(|x| moment(x, time)).unwrap_or(0);
        }
        let mut past = 0;
        for year in today.year() - 1..=today.year() + 1 {
            let Some(r) = on(year).and_then(|x| moment(x, time)) else {
                continue;
            };
            if r >= now - SLACK && r <= now + span + SLACK {
                return r;
            }
            if r < now {
                past = r;
            }
        }
        return past;
    }
    let Some(time) = hour_minute(s) else {
        return 0;
    };
    (0..=2)
        .filter_map(|d: i64| today.checked_add(d.days()).ok())
        .filter_map(|date| moment(date, time))
        .find(|r| *r > now)
        .unwrap_or(0)
}

fn hour_minute(text: &str) -> Option<(i8, i8)> {
    let m = clock().captures(text.trim())?;
    let pm = m[3].eq_ignore_ascii_case("pm");
    let hour = m[1].parse::<i8>().ok()? % 12 + if pm { 12 } else { 0 };
    let minute = m.get(2).map_or(Some(0), |x| x.as_str().parse().ok())?;
    Some((hour, minute))
}

#[cfg(test)]
#[path = "usage_text_tests.rs"]
mod tests;
