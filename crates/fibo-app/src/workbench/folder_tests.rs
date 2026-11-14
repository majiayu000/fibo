//! Which folder a terminal starts in and how a space's folder reads, on this machine and on
//! another one (`attach::Host`), whose folders only its own host can check.
use super::*;

/// This machine's host.
fn here() -> crate::attach::Host {
    crate::attach::Host::default()
}

#[test]
fn a_removed_directory_falls_back_to_one_that_exists() {
    let gone = "/nonexistent/mooring-idle-test/next".to_owned();
    let tmp = std::env::temp_dir().to_string_lossy().into_owned();
    assert_eq!(usable_cwd(&here(), [gone.clone(), tmp.clone()]), tmp);
    assert_eq!(
        usable_cwd(&here(), [gone.clone(), gone]),
        std::env::var("HOME").unwrap()
    );
}
#[test]
fn a_space_without_a_folder_is_not_missing() {
    let space = |cwd: &str| Space {
        id: "s".into(),
        name: "s".into(),
        cwd: cwd.into(),
    };
    assert!(!space_dir_missing(&here(), &space("")));
    assert!(!space_dir_missing(&here(), &space("/")));
    assert!(space_dir_missing(
        &here(),
        &space("/nonexistent/mooring-gone")
    ));
    assert_eq!(
        space_dir_label(&here(), &space(""), ""),
        "未设目录 · 从 ~ 开始"
    );
    assert_eq!(
        space_dir_label(&here(), &space(""), "/tmp"),
        "从上次的 /tmp 开始"
    );
    assert_eq!(
        space_dir_label(&here(), &space("/tmp"), "/elsewhere"),
        "/tmp"
    );
    assert_eq!(
        usable_cwd(&here(), [String::new()]),
        std::env::var("HOME").unwrap()
    );
}
