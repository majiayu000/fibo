use super::*;
use LaunchIssue::*;

fn window() -> PathBuf {
    PathBuf::from("/Applications/Fibo.app/Contents/MacOS/fibo-app")
}
fn started_by_window() -> Launch {
    Launch {
        remote_session: Some(false),
        graphic_session: Some(true),
        nice: 0,
        responsible: Some(window()),
    }
}

#[test]
fn a_host_the_window_started_has_nothing_off() {
    assert_eq!(issues(&started_by_window(), &window()), vec![]);
    // A platform that knows nothing reports nothing.
    assert_eq!(issues(&Launch::default(), &window()), vec![]);
}

/// 11-16: `ssh agentos 'nohup fibo serve --takeover &'` from the other Mac.
#[test]
fn a_host_started_over_ssh_with_a_shells_ampersand_has_all_three() {
    let ssh = Launch {
        remote_session: Some(true),
        graphic_session: Some(false),
        nice: 5,
        responsible: Some(PathBuf::from("/usr/libexec/sshd-session")),
    };
    assert_eq!(
        issues(&ssh, &window()),
        vec![RemoteSession, LowPriority, NotFromWindow]
    );
    // A session without the screen is as bad as a remote one for the keychain.
    let background = Launch {
        remote_session: Some(false),
        graphic_session: Some(false),
        ..started_by_window()
    };
    assert_eq!(issues(&background, &window()), vec![RemoteSession]);
}

#[test]
fn only_what_the_window_does_not_share_is_fixed_by_restarting() {
    let host = [RemoteSession, LowPriority, NotFromWindow];
    assert_eq!(fixed_by_restart(&host, &[]), host.to_vec());
    // A window run from a terminal is not responsible for itself: neither is its host.
    assert_eq!(
        fixed_by_restart(&host, &[NotFromWindow]),
        vec![RemoteSession, LowPriority]
    );
    assert_eq!(fixed_by_restart(&[Other], &[]), vec![]);
}

#[test]
fn an_issue_this_build_does_not_know_still_reads() {
    let read: Vec<LaunchIssue> =
        serde_json::from_str(r#"["remote_session","something_newer"]"#).unwrap();
    assert_eq!(read, vec![RemoteSession, Other]);
}

/// Both ways across a version: a host from before `launch`, and a window from before it.
#[test]
fn a_build_reads_without_launch_and_writes_none_when_fine() {
    let old: crate::protocol::Build =
        serde_json::from_str(r#"{"binary":"/x/fibo","digest":"d","pid":7,"handoff":true}"#)
            .unwrap();
    assert!(old.launch.is_empty());
    let fine = serde_json::to_string(&old).unwrap();
    assert!(!fine.contains("launch"), "{fine}");
    let off = crate::protocol::Build {
        launch: vec![RemoteSession],
        ..old
    };
    let text = serde_json::to_string(&off).unwrap();
    assert!(text.contains(r#""launch":["remote_session"]"#), "{text}");
}
