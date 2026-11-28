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

/// What stays on as `moving` once `saved` becomes `next`: every move not yet done, and the
/// old main relay when it was replaced and phones were paired through it, less any relay in
/// use again. Moves are kept apart: a phone off since A→B knows only A, one that came on
/// before B→C knows B, so changing twice keeps both until each has its phones.
pub(crate) fn after_save(saved: &Config, next: &Config, now: u64, paired: bool) -> Vec<Moving> {
    let in_use = |r: &str| r == next.relay || next.also.iter().any(|a| a.relay == r);
    let mut moving: Vec<Moving> = (saved.moving.iter())
        .filter(|m| !in_use(&m.relay))
        .cloned()
        .collect();
    if paired && !saved.relay.is_empty() && !in_use(&saved.relay) {
        moving.retain(|m| m.relay != saved.relay);
        moving.push(Moving {
            relay: saved.relay.clone(),
            key: saved.key.clone(),
            since: now,
        });
    }
    moving
}

/// Read as one move or a list: builds from 11-28 kept one; one is written as one, so such a
/// build still reads the file while only one move is going.
pub(crate) mod one_or_many {
    use super::Moving;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Form {
        One(Moving),
        Many(Vec<Moving>),
    }

    pub fn serialize<S: Serializer>(moving: &[Moving], s: S) -> Result<S::Ok, S::Error> {
        match moving {
            [one] => one.serialize(s),
            many => many.serialize(s),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Moving>, D::Error> {
        Ok(match Option::<Form>::deserialize(d)? {
            None => vec![],
            Some(Form::One(one)) => vec![one],
            Some(Form::Many(many)) => many,
        })
    }
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
            moving: vec![],
        }
    }

    #[test]
    fn a_replaced_main_relay_stays_until_it_is_in_use_again() {
        let a = config("https://a", &[]);
        let moved = after_save(&a, &config("https://c", &[]), 100, true);
        assert_eq!(moved.len(), 1);
        assert_eq!(
            (moved[0].relay.as_str(), moved[0].since),
            ("https://a", 100)
        );
        assert_eq!(moved[0].key.as_deref(), Some("key-https://a"));
        // No phones paired: nothing to move.
        assert_eq!(
            after_save(&a, &config("https://c", &[]), 100, false),
            vec![]
        );
        // Kept as the backup, or swapped with it: still in use.
        assert_eq!(
            after_save(&a, &config("https://c", &["https://a"]), 100, true),
            vec![]
        );
        // A later save that leaves the main relay alone keeps the move going.
        let c = Config {
            moving: moved.clone(),
            ..config("https://c", &[])
        };
        assert_eq!(
            after_save(&c, &config("https://c", &["https://b"]), 200, true),
            moved
        );
        // Moving back to the old relay ends that move and starts one off C.
        let back = after_save(&c, &config("https://a", &[]), 200, true);
        assert_eq!(
            back.iter().map(|m| m.relay.as_str()).collect::<Vec<_>>(),
            ["https://c"]
        );
    }

    /// PR 675 review: A→B, then B→C before a phone that knows only A came back, must keep A
    /// on, and B for phones that learned it in between.
    #[test]
    fn changing_twice_keeps_every_unfinished_move() {
        let a = config("https://a", &[]);
        let b = Config {
            moving: after_save(&a, &config("https://b", &[]), 100, true),
            ..config("https://b", &[])
        };
        let c = after_save(&b, &config("https://c", &[]), 200, true);
        assert_eq!(
            c.iter()
                .map(|m| (m.relay.as_str(), m.since))
                .collect::<Vec<_>>(),
            [("https://a", 100), ("https://b", 200)]
        );
        let saved = Config {
            moving: c.clone(),
            ..config("https://c", &[])
        };
        let dialed: Vec<_> = saved.extra().into_iter().map(|r| r.relay).collect();
        assert_eq!(dialed, ["https://a", "https://b"]);
    }

    #[test]
    fn one_move_is_written_as_one_and_either_form_is_read() {
        let one = Config {
            moving: after_save(
                &config("https://a", &[]),
                &config("https://b", &[]),
                100,
                true,
            ),
            ..config("https://b", &[])
        };
        let json = serde_json::to_value(&one).unwrap();
        assert!(json["moving"].is_object(), "{json}");
        assert_eq!(serde_json::from_value::<Config>(json).unwrap(), one);
        let two = Config {
            moving: after_save(&one, &config("https://c", &[]), 200, true),
            ..config("https://c", &[])
        };
        let json = serde_json::to_value(&two).unwrap();
        assert!(json["moving"].is_array(), "{json}");
        assert_eq!(serde_json::from_value::<Config>(json).unwrap(), two);
        let none = config("https://c", &[]);
        let json = serde_json::to_value(&none).unwrap();
        assert!(json.get("moving").is_none(), "{json}");
        assert_eq!(serde_json::from_value::<Config>(json).unwrap(), none);
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
