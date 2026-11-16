use super::Build;

#[test]
fn a_host_is_stale_only_when_the_code_differs() {
    let dir = std::env::temp_dir().join(format!("kl-build-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let binary = dir.join("fibo");
    std::fs::write(&binary, "v1").unwrap();
    let started = Build {
        binary: binary.to_string_lossy().into_owned(),
        digest: Build::digest(&binary).unwrap(),
        pid: 1,
        handoff: true,
        guarded_chat: true,
        launch: vec![],
    };
    assert!(!started.stale(&dir.join("absent")));
    // A copy with other timestamps (re-signed, touched) is the same build.
    let copy = dir.join("bundled");
    std::fs::write(&copy, "v1").unwrap();
    assert!(!started.stale(&copy));
    std::fs::write(&copy, "v2").unwrap();
    assert!(started.stale(&copy));
    assert!(Build::default().stale(&binary), "hosts before digests");
    std::fs::remove_dir_all(dir).unwrap();
}
