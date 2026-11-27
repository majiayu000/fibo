//! `remote-relay.json` that cannot be understood: the setting says so and is never saved
//! over, since that would drop the relays' keys.
use super::remote::{Config, Supervisor};
use fibo_core::protocol::{RemoteRelay, RemoteState};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("fibo-relay-setting-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_relay_setting_that_cannot_be_read_says_why_and_is_not_saved_over() {
    let root = scratch("broken");
    let file = root.join("remote-relay.json");
    let broken = br#"{"enabled": true, "relay": "wss://relay.example", "key": "kept"#;
    std::fs::write(&file, broken).unwrap();

    let remote = Supervisor::start(&root, || {});
    let live = remote.status().live;
    assert_eq!(live.state, RemoteState::Failed, "{live:?}");
    assert!(live.detail.contains("remote-relay.json"), "{live:?}");

    // What settings send when the switch moves: no keys, meaning 「keep the ones there」.
    let refused = remote.configure(false, "wss://relay.example".into(), None, None);
    let error = refused.expect_err("saving over it would lose the keys");
    assert!(error.to_string().contains("remote-relay.json"), "{error}");
    assert_eq!(std::fs::read(&file).unwrap(), broken);
}

#[test]
fn a_missing_relay_setting_is_off_and_a_saved_one_keeps_its_keys() {
    let root = scratch("fresh");
    let remote = Supervisor::start(&root, || {});
    assert_eq!(remote.status().live.state, RemoteState::Off);

    remote
        .configure(
            false,
            "wss://relay.example".into(),
            Some("kept".into()),
            Some(vec![RemoteRelay {
                relay: "wss://second.example".into(),
                key: Some("also kept".into()),
            }]),
        )
        .unwrap();
    let saved: Config =
        serde_json::from_slice(&std::fs::read(root.join("remote-relay.json")).unwrap()).unwrap();
    assert_eq!(saved.key.as_deref(), Some("kept"));
    assert_eq!(saved.also[0].key.as_deref(), Some("also kept"));
    assert_eq!(remote.key().as_deref(), Some("kept"));
}

#[test]
fn saving_after_an_external_repair_keeps_the_recovered_keys() {
    let root = scratch("repaired");
    let file = root.join("remote-relay.json");
    std::fs::write(&file, b"not JSON").unwrap();
    let remote = Supervisor::start(&root, || {});
    assert_eq!(remote.status().live.state, RemoteState::Failed);

    let repaired = Config {
        enabled: true,
        relay: "wss://relay.example".into(),
        key: Some("recovered".into()),
        also: vec![RemoteRelay {
            relay: "wss://second.example".into(),
            key: Some("second recovered".into()),
        }],
    };
    std::fs::write(&file, serde_json::to_vec(&repaired).unwrap()).unwrap();

    remote
        .configure(false, repaired.relay.clone(), None, None)
        .unwrap();
    let saved: Config = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(
        saved,
        Config {
            enabled: false,
            ..repaired
        }
    );
    assert_eq!(remote.key().as_deref(), Some("recovered"));
    assert_eq!(remote.status().live.state, RemoteState::Off);
}
