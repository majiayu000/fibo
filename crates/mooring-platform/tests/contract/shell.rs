use mooring_platform::shell::{SYSTEM_PATH, sh};
use std::process::Command;

#[test]
fn sh_runs_a_posix_command() {
    let out = Command::new(sh())
        .args(["-c", r#"a=mo; printf '%s' "${a}oring""#])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "mooring");
}

#[test]
fn the_system_path_splits_into_absolute_directories() {
    let dirs: Vec<_> = std::env::split_paths(SYSTEM_PATH).collect();
    assert!(!dirs.is_empty());
    assert!(dirs.iter().all(|d| d.is_absolute()), "{dirs:?}");
}
