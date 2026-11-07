//! One CLI's reading out of all it has. A status line and a `/usage` read can both feed Claude,
//! and only the second names one model's week: taking the newest whole would make that window
//! come and go at every status line refresh.
use crate::quota::Quota;
use std::collections::HashMap;

/// How much older a reading may be and still add a window to the newest: past this it says
/// nothing of the same moment, and the reading it would sit under would claim a fresher time.
const KEEPS_FOR: u64 = 3600;

/// The newest of `readings` (the first of a tie), with the named windows only other recent ones
/// know. A window without a name is the newest reading's to say: that it left one out means
/// something, and a session's old reading must not bring it back.
pub(crate) fn newest(readings: &[&Quota], now: u64) -> Quota {
    let mut out = Quota::default();
    for q in readings {
        if out.windows.is_empty() || q.at > out.at {
            out = (*q).clone();
        }
    }
    let at = out.at;
    for q in readings {
        if at.saturating_sub(q.at) > KEEPS_FOR {
            continue;
        }
        for w in &q.windows {
            let known = (out.windows.iter()).any(|o| o.minutes == w.minutes && o.name == w.name);
            if !w.name.is_empty() && !known && !w.reset(now) {
                out.windows.push(w.clone());
            }
        }
    }
    // Shortest first as `Quota` promises, the account's own before a named one of its length.
    (out.windows).sort_by_key(|w| (w.minutes, !w.name.is_empty()));
    out
}

/// What a command source that answered leaves: its reading, or, having no window to tell
/// (switched off, an API key), none, so what it said before stops showing.
pub(crate) fn settle(readings: &mut HashMap<usize, Quota>, source: usize, reading: Option<Quota>) {
    match reading {
        Some(q) => readings.insert(source, q),
        None => readings.remove(&source),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{Source, Window};

    fn window(minutes: u32, used: u16, name: &str) -> Window {
        Window {
            minutes,
            used,
            resets_at: 9_000,
            name: name.into(),
        }
    }
    fn reading(at: u64, windows: Vec<Window>) -> Quota {
        Quota {
            at,
            plan: String::new(),
            windows,
        }
    }
    fn shape(q: &Quota) -> Vec<(String, u16)> {
        q.windows.iter().map(|w| (w.label(), w.used)).collect()
    }

    /// The Fable week stays while status lines, which do not know it, keep arriving.
    #[test]
    fn a_named_window_survives_a_newer_reading_that_does_not_know_it() {
        let usage = reading(
            1000,
            vec![
                window(300, 10, ""),
                window(10_080, 20, ""),
                window(10_080, 50, "Fable"),
            ],
        );
        let status = reading(1500, vec![window(300, 12, ""), window(10_080, 21, "")]);
        let got = newest(&[&usage, &status], 2000);
        assert_eq!(got.at, 1500);
        assert_eq!(
            shape(&got),
            [
                ("5h".into(), 12),
                ("周".into(), 21),
                ("Fable 周".into(), 50)
            ]
        );
        // Either order, and the newer named reading's number is the one said.
        assert_eq!(newest(&[&status, &usage], 2000), got);
        let later = reading(1600, vec![window(10_080, 60, "Fable")]);
        let got = newest(&[&usage, &status, &later], 2000);
        assert_eq!(shape(&got), [("Fable 周".to_owned(), 60)]);
    }

    #[test]
    fn what_is_too_old_or_past_its_reset_adds_nothing() {
        let status = reading(10_000, vec![window(300, 12, "")]);
        let old = reading(10_000 - KEEPS_FOR - 1, vec![window(10_080, 50, "Fable")]);
        assert_eq!(shape(&newest(&[&status, &old], 10_001)).len(), 1);
        let renewed = reading(9_999, vec![window(10_080, 50, "Fable")]);
        assert_eq!(shape(&newest(&[&status, &renewed], 10_001)).len(), 1);
    }

    /// The rule readings had before any had a name: the newest whole.
    #[test]
    fn a_window_without_a_name_is_only_the_newest_readings_to_say() {
        let old = reading(1000, vec![window(300, 10, ""), window(10_080, 20, "")]);
        let new = reading(1500, vec![window(10_080, 21, "")]);
        assert_eq!(shape(&newest(&[&old, &new], 2000)), [("周".to_owned(), 21)]);
    }

    #[test]
    fn a_source_with_no_window_left_stops_showing_what_it_said() {
        let mut readings = HashMap::new();
        settle(&mut readings, 0, Some(reading(1, vec![window(300, 1, "")])));
        settle(&mut readings, 1, Some(reading(1, vec![window(300, 2, "")])));
        settle(&mut readings, 0, None);
        assert_eq!(readings.keys().collect::<Vec<_>>(), [&1]);
    }

    /// `claude-quota`'s declaration against the document `mooring claude-quota usage` prints.
    #[test]
    fn the_shipped_usage_source_reads_a_model_window_with_its_name() {
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins/claude-quota/quota.json");
        let file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(shipped).unwrap()).unwrap();
        let source: Source = serde_json::from_value(file["sources"][1].clone()).unwrap();
        // Six seconds usually, and over 20 on a busy machine: the default would cut it off.
        assert!(source.timeout_ms.is_some_and(|t| t >= 60_000));
        let doc = serde_json::json!({"at": 500,
            "session": {"used": 2, "resets_at": 900},
            "week": {"used": 19, "resets_at": 901},
            "models": [{"name": "Fable", "used": 3, "resets_at": 902}]});
        let q = source.reading(&doc, 1).unwrap();
        assert_eq!(q.at, 500);
        assert_eq!(
            shape(&q),
            [
                ("5h".to_owned(), 2),
                ("周".to_owned(), 19),
                ("Fable 周".to_owned(), 3)
            ]
        );
        // No window: the reading is none, which is what clears it.
        assert_eq!(source.reading(&serde_json::json!({"at": 500}), 1), None);
    }

    /// A reading stored before names existed reads back, and one without a name stores as before.
    #[test]
    fn a_window_stored_before_names_existed_still_reads() {
        let old: Window =
            serde_json::from_str(r#"{"minutes":300,"used":4,"resets_at":9}"#).unwrap();
        assert_eq!(old.name, "");
        assert!(!serde_json::to_string(&old).unwrap().contains("name"));
    }

    #[test]
    fn a_named_window_takes_the_bar_only_near_its_end_and_is_no_waste() {
        let named = |used| window(10_080, used, "Fable");
        assert!(window(10_080, 0, "").on_bar());
        assert!(!named(79).on_bar());
        assert!(named(80).on_bar());
        let renews_soon = Window {
            resets_at: 1000,
            ..named(0)
        };
        assert_eq!(renews_soon.spare(900), None);
        let account = Window {
            name: String::new(),
            ..renews_soon
        };
        assert_eq!(account.spare(900), Some(100));
    }
}
