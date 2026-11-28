//! A main relay replaced in the setting stays on as `moving` until every device paired
//! before the change has connected once: each learns the new relays from `hello` then.
//! Without it a phone that only knows the old address would never find the machine again.
use super::Config;
use crate::devices::Device;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Moving {
    pub relay: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// When it was replaced, in Unix seconds.
    pub since: u64,
}

/// What stays on as `moving` once `saved` becomes `next`: the old main relay when it was
/// replaced and phones were paired through it, else what was moving, unless it is in use
/// again.
pub(crate) fn after_save(saved: &Config, next: &Config, now: u64, paired: bool) -> Option<Moving> {
    let in_use = |r: &str| r == next.relay || next.also.iter().any(|a| a.relay == r);
    if paired && !saved.relay.is_empty() && !in_use(&saved.relay) {
        return Some(Moving {
            relay: saved.relay.clone(),
            key: saved.key.clone(),
            since: now,
        });
    }
    saved.moving.clone().filter(|m| !in_use(&m.relay))
}

/// Devices paired before the move (or in its second) that have not connected since.
pub(crate) fn left(moving: &Moving, devices: &[Device], seen: &HashMap<String, u64>) -> usize {
    (devices.iter())
        .filter(|d| d.paired <= moving.since)
        .filter(|d| seen.get(&d.id).is_none_or(|&at| at < moving.since))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibo_core::protocol::RemoteRelay;

    fn config(relay: &str, also: &[&str]) -> Config {
        Config {
            enabled: true,
            relay: relay.into(),
            key: Some(format!("key-{relay}")),
            also: also
                .iter()
                .map(|r| RemoteRelay {
                    relay: (*r).into(),
                    key: None,
                })
                .collect(),
            moving: None,
        }
    }

    #[test]
    fn a_replaced_main_relay_stays_until_it_is_in_use_again() {
        let a = config("https://a", &[]);
        let moved = after_save(&a, &config("https://c", &[]), 100, true).unwrap();
        assert_eq!((moved.relay.as_str(), moved.since), ("https://a", 100));
        assert_eq!(moved.key.as_deref(), Some("key-https://a"));
        // No phones paired: nothing to move.
        assert_eq!(after_save(&a, &config("https://c", &[]), 100, false), None);
        // Kept as the backup, or swapped with it: still in use.
        assert_eq!(
            after_save(&a, &config("https://c", &["https://a"]), 100, true),
            None
        );
        // A later save that leaves the main relay alone keeps the move going.
        let c = Config {
            moving: Some(moved.clone()),
            ..config("https://c", &[])
        };
        assert_eq!(
            after_save(&c, &config("https://c", &["https://b"]), 200, true),
            Some(moved)
        );
        // Moving back to the old relay ends it.
        assert!(after_save(&c, &config("https://a", &[]), 200, true)
            .is_some_and(|m| m.relay == "https://c"));
    }

    #[test]
    fn devices_paired_before_the_move_must_connect_after_it() {
        let device = |id: &str, paired: u64| -> Device {
            serde_json::from_value(serde_json::json!({
                "id": id, "name": id, "key": "", "role": "full", "paired": paired,
            }))
            .unwrap()
        };
        let moving = Moving {
            relay: "https://a".into(),
            key: None,
            since: 100,
        };
        let devices = [device("old", 100), device("seen", 10), device("new", 150)];
        let seen = HashMap::from([("seen".to_owned(), 120), ("old".to_owned(), 90)]);
        assert_eq!(left(&moving, &devices, &seen), 1);
        let seen = HashMap::from([("seen".to_owned(), 120), ("old".to_owned(), 101)]);
        assert_eq!(left(&moving, &devices, &seen), 0);
    }
}
