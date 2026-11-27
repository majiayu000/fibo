//! The account's limits out of every conversation's status line. Each running Claude Code
//! hands out the limits it last heard, so a conversation idle for an hour still reports the
//! 10% of then after a busy one reported 19%: the last to write is not the newest. Within one
//! window (the same `resets_at`) use only goes up, so the larger is the newer; a later
//! `resets_at` is a new window and replaces the old whatever its number.
//!
//! Not covered: the payload names no account, so two accounts' conversations at once would
//! show the larger of the two; and a correction the server makes downward within a window
//! shows only once the window resets.
use serde_json::{Map, Value};

pub(super) const WINDOWS: [&str; 2] = ["five_hour", "seven_day"];

/// `rate_limits` to keep: `new`'s, with each window `old` knows of the same or a later reset
/// and a larger use kept from `old`.
pub(super) fn merge(old: Option<&Value>, new: &Value) -> Value {
    let mut out: Map<String, Value> = new.as_object().cloned().unwrap_or_default();
    for key in WINDOWS {
        let Some(kept) = old.and_then(|o| o.get(key)) else {
            continue;
        };
        let keep = match out.get(key) {
            None => true,
            Some(came) => newer(kept, came),
        };
        if keep {
            out.insert(key.to_owned(), kept.clone());
        }
    }
    Value::Object(out)
}

/// Whether `kept` says more of now than `came`: a later window, or more used in the same one.
fn newer(kept: &Value, came: &Value) -> bool {
    let reset = |w: &Value| w["resets_at"].as_f64();
    let used = |w: &Value| w["used_percentage"].as_f64().unwrap_or(0.0);
    match (reset(kept), reset(came)) {
        (Some(k), Some(c)) if k != c => k > c,
        (Some(_), Some(_)) => used(kept) > used(came),
        // A window with no reset time says nothing of which is newer: what came is.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn limits(five: (u64, u64), week: (u64, u64)) -> Value {
        json!({
            "five_hour": { "used_percentage": five.0, "resets_at": five.1 },
            "seven_day": { "used_percentage": week.0, "resets_at": week.1 },
        })
    }
    fn used(v: &Value) -> (u64, u64) {
        let u = |k: &str| v[k]["used_percentage"].as_u64().unwrap();
        (u("five_hour"), u("seven_day"))
    }

    /// 19 → 10 → 19 as seen on 11-16: an idle conversation's old 10 no longer shows.
    #[test]
    fn an_older_conversations_lower_reading_in_the_same_window_does_not_come_back() {
        let kept = limits((19, 1000), (99, 9000));
        let came = limits((10, 1000), (28, 9000));
        assert_eq!(used(&merge(Some(&kept), &came)), (19, 99));
        let more = limits((22, 1000), (99, 9000));
        assert_eq!(used(&merge(Some(&kept), &more)), (22, 99));
    }

    /// A new five hours starts low; an old conversation still in the last one does not undo it.
    #[test]
    fn a_new_window_replaces_the_old_and_the_old_one_cannot_return() {
        let kept = limits((90, 1000), (50, 9000));
        let fresh = limits((2, 19_000), (51, 9000));
        let now = merge(Some(&kept), &fresh);
        assert_eq!(used(&now), (2, 51));
        assert_eq!(used(&merge(Some(&now), &kept)), (2, 51));
    }

    #[test]
    fn a_window_only_one_side_has_is_kept_and_nothing_old_starts_fresh() {
        let kept = limits((19, 1000), (99, 9000));
        let week_only = json!({ "seven_day": { "used_percentage": 99, "resets_at": 9000 } });
        assert_eq!(used(&merge(Some(&kept), &week_only)), (19, 99));
        assert_eq!(used(&merge(None, &kept)), (19, 99));
    }
}
