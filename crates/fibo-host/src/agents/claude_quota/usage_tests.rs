use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};

struct Sandbox(std::path::PathBuf);
impl Sandbox {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("fibo-quota-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn read(&self, at: u64, scope: &str, result: Result<Value, oauth::Failure>) -> Value {
        collect(&self.0, false, at, || Ok(credential(scope)), |_| result).unwrap()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn credential(scope: &str) -> oauth::Credential {
    oauth::Credential {
        token: "not-saved".into(),
        scope: scope.into(),
    }
}
fn quota(used: u64, reset: u64) -> Value {
    json!({"session":{"used":used,"resets_at":reset},"week":{"used":37,"resets_at":9000},"models":[]})
}

#[test]
fn old_statusline_and_cli_files_cannot_replace_http_or_renew_its_time() {
    let s = Sandbox::new();
    let first = s.read(1000, "a", Ok(quota(19, 2000)));
    for used in [10, 19, 18, 22, 17] {
        std::fs::write(s.0.join("statusline.json"), quota(used, 2000).to_string()).unwrap();
        std::fs::write(s.0.join("usage.json"), quota(used, 2000).to_string()).unwrap();
        let got = collect(
            &s.0,
            false,
            1001,
            || Ok(credential("a")),
            |_| panic!("cache hit must not query"),
        )
        .unwrap();
        assert_eq!(got, first);
    }
    let text = std::fs::read_to_string(s.0.join("http-usage.json")).unwrap();
    assert!(!text.contains("not-saved"));
    assert_eq!(first["at"], 1000);
}

#[test]
fn a_successful_lower_measurement_is_accepted_in_the_same_window() {
    let s = Sandbox::new();
    s.read(1000, "a", Ok(quota(19, 2000)));
    let got = s.read(1120, "a", Ok(quota(10, 2000)));
    assert_eq!(got["session"]["used"], 10);
    assert_eq!(got["at"], 1120);
}

#[test]
fn reset_bypasses_normal_cache_ttl_and_week_remains_independent() {
    let s = Sandbox::new();
    s.read(1000, "a", Ok(quota(19, 1050)));
    let got = s.read(1050, "a", Ok(quota(2, 2050)));
    assert_eq!(got["session"]["used"], 2);
    assert_eq!(got["session"]["resets_at"], 2050);
    assert_eq!(got["week"]["used"], 37);
    // A response still carrying the expired window stays expired, never a made-up new 0%.
    let late = s.read(2050, "a", Ok(quota(100, 2050)));
    assert_eq!(late["session"]["used"], 100);
    assert_eq!(late["session"]["resets_at"], 2050);
    collect(
        &s.0,
        false,
        2051,
        || Ok(credential("a")),
        |_| panic!("past reset must not cause a retry loop"),
    )
    .unwrap();
}

#[test]
fn failure_keeps_the_sample_time_and_survives_a_new_reader() {
    let s = Sandbox::new();
    s.read(1000, "a", Ok(quota(19, 2000)));
    let failed = s.read(1120, "a", Err(oauth::Failure::read("offline")));
    assert_eq!(failed["at"], 1000);
    assert_eq!(failed["session"]["used"], 19);
    assert_eq!(failed["error"], "offline");
    let again = collect(
        &s.0,
        false,
        1121,
        || Ok(credential("a")),
        |_| panic!("retry cooldown"),
    )
    .unwrap();
    assert_eq!(again, failed);
    let recovered = s.read(1240, "a", Ok(quota(20, 2000)));
    assert!(recovered.get("error").is_none());
    assert_eq!(recovered["at"], 1240);
}

#[test]
fn manual_refresh_honors_server_cooldown_across_restart_and_reset() {
    let s = Sandbox::new();
    s.read(1000, "a", Ok(quota(19, 1100)));
    s.read(
        1120,
        "a",
        Err(oauth::Failure {
            message: "limited".into(),
            retry_at: Some(1800),
            discard: false,
        }),
    );
    collect(
        &s.0,
        true,
        1799,
        || Ok(credential("a")),
        |_| panic!("Retry-After applies to manual too"),
    )
    .unwrap();
    assert_eq!(s.read(1800, "a", Ok(quota(2, 2800)))["session"]["used"], 2);
}

#[test]
fn another_credential_never_inherits_a_cache_even_when_its_query_fails() {
    let s = Sandbox::new();
    s.read(1000, "a", Ok(quota(19, 2000)));
    let other = s.read(1001, "b", Err(oauth::Failure::read("offline")));
    assert!(other.get("session").is_none());
    assert_eq!(other["at"], 0);
    let missing = collect(&s.0, false, 1002, || Err("signed out".into()), |_| panic!()).unwrap();
    assert!(missing.get("week").is_none());
    assert_eq!(missing["error"], "signed out");
}

#[test]
fn a_credential_change_during_http_discards_the_response() {
    let s = Sandbox::new();
    s.read(1000, "a", Ok(quota(19, 2000)));
    let mut reads = 0;
    let got = collect(
        &s.0,
        true,
        1001,
        || {
            reads += 1;
            Ok(credential(if reads == 1 { "a" } else { "b" }))
        },
        |_| Ok(quota(99, 2000)),
    )
    .unwrap();
    assert!(got.get("session").is_none());
    assert!(got["error"].is_string());
    let new = s.read(1002, "b", Ok(quota(2, 2000)));
    assert_eq!(new["session"]["used"], 2);
}

#[test]
fn authentication_rejection_clears_previous_quota() {
    let s = Sandbox::new();
    s.read(1000, "a", Ok(quota(19, 2000)));
    let got = s.read(
        1120,
        "a",
        Err(oauth::Failure {
            message: "unauthorized".into(),
            retry_at: None,
            discard: true,
        }),
    );
    assert!(got.get("session").is_none());
    assert_eq!(got["error"], "unauthorized");
}

#[test]
fn concurrent_readers_share_one_request_and_never_publish_out_of_order() {
    let s = Sandbox::new();
    s.read(1000, "a", Ok(quota(19, 2000)));
    let dir = s.0.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let worker = std::thread::spawn(move || {
        let mut first = true;
        collect(
            &dir,
            false,
            1120,
            || {
                // Hold before the cooldown is written: only the OS lock can keep
                // another reader from issuing a duplicate request in this interval.
                if first {
                    first = false;
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }
                Ok(credential("a"))
            },
            |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(quota(22, 2000))
            },
        )
        .unwrap()
    });
    started_rx.recv().unwrap();
    let during = collect(
        &s.0,
        false,
        1121,
        || Ok(credential("a")),
        |_| panic!("second HTTP request"),
    )
    .unwrap();
    assert_eq!(during["session"]["used"], 19);
    assert_eq!(during["at"], 1000);
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap()["session"]["used"], 22);
    let after = collect(
        &s.0,
        false,
        1122,
        || Ok(credential("a")),
        |_| panic!("cache miss"),
    )
    .unwrap();
    assert_eq!(after["session"]["used"], 22);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn corrupt_cache_is_reported_without_overwriting_it() {
    let s = Sandbox::new();
    let p = s.0.join("http-usage.json");
    std::fs::write(&p, "{broken").unwrap();
    assert!(collect(&s.0, false, 1000, || Ok(credential("a")), |_| panic!()).is_err());
    assert_eq!(std::fs::read_to_string(p).unwrap(), "{broken");
}
