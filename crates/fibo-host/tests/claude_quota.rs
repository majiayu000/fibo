//! `fibo claude-quota`, the program itself against a fake `claude`: the status line, setting
//! it up over what was there. HTTP, account switches and cache races are tested beside usage.rs.
#![cfg(unix)]
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_fibo");

#[test]
fn quota_errors_follow_the_hosts_language_without_loading_all_plugins() {
    let s = Sandbox::new("quota-english");
    let out = s
        .command("usage")
        .env("MOORING_LANG", "en")
        .output()
        .unwrap();
    assert!(out.status.success());
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        doc["error"]
            .as_str()
            .unwrap()
            .starts_with("No readable Claude"),
        "{doc}"
    );
    assert_eq!(doc["at"], 0);
}

struct Sandbox {
    root: PathBuf,
    data: PathBuf,
}
impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let root = std::env::temp_dir().join(format!("mcq-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (data, bin) = (root.join("data"), root.join("bin"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(root.join("home/.claude/projects/p")).unwrap();
        Sandbox { root, data }
    }
    fn command(&self, what: &str) -> Command {
        let mut c = Command::new(BIN);
        c.args(["claude-quota", what])
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("HOME", self.root.join("home"))
            .env("CLAUDE_CONFIG_DIR", self.root.join("home/.claude"))
            .env("CLAUDECODE", "1")
            .env("MOORING_PLUGIN_DATA", &self.data);
        c
    }
    fn tick(&self, on: bool) -> Output {
        self.command("usage")
            .env("MOORING_OPTION_USAGE", if on { "1" } else { "0" })
            .output()
            .unwrap()
    }
    fn windows(out: &Output) -> Value {
        assert!(out.status.success(), "{out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn settings(&self) -> PathBuf {
        self.root.join("home/.claude/settings.json")
    }
    fn read_settings(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.settings()).unwrap()).unwrap()
    }
    fn run_ok(&self, what: &str) -> String {
        let out = self.command(what).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    }
    fn status(&self, payload: &str) -> String {
        let mut child = Command::new(BIN)
            .args(["claude-quota", "status"])
            .arg(&self.data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

// The status line.
fn context_payload(id: &str, read: u64) -> String {
    json!({"session_id": id, "context_window": {"context_window_size": 1_000_000,
        "current_usage": {"input_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": read}}})
    .to_string()
}

#[test]
fn each_conversation_keeps_its_own_reading_and_the_old_status_line_still_prints() {
    use fibo_core::context_fill::from_claude_status;
    let s = Sandbox::new("contexts");
    std::fs::write(s.data.join("original.sh"), "cat >/dev/null; echo mine\n").unwrap();
    assert_eq!(s.status(&context_payload("aaa-1", 600_000)), "mine\n");
    assert_eq!(s.status(&context_payload("bbb-2", 900_000)), "mine\n");
    let percent = |id: &str| {
        let doc = std::fs::read(s.data.join(format!("contexts/{id}.json"))).unwrap();
        from_claude_status(&serde_json::from_slice(&doc).unwrap())
            .unwrap()
            .percent()
    };
    assert_eq!((percent("aaa-1"), percent("bbb-2")), (60, 90));
    assert_eq!(s.status(&context_payload("aaa-1", 700_000)), "mine\n");
    assert_eq!(percent("aaa-1"), 70);
    let contexts = s.data.join("contexts");
    // Nothing but the readings is left, and an id that could leave the folder writes nothing.
    assert_eq!(std::fs::read_dir(&contexts).unwrap().count(), 2);
    assert_eq!(s.status(&context_payload("../escape", 1)), "mine\n");
    assert!(!s.data.join("escape.json").exists());
    assert_eq!(std::fs::read_dir(&contexts).unwrap().count(), 2);
    assert_eq!(mode(&contexts.join("aaa-1.json")), 0o600);
}

#[test]
fn statusline_keeps_printing_but_never_publishes_account_quota() {
    let s = Sandbox::new("limits");
    std::fs::write(s.data.join("original.sh"), "cat; echo\n").unwrap();
    let kept = s.data.join("statusline.json");
    assert_eq!(
        s.status(r#"{"session_id":"a"}"#),
        "{\"session_id\":\"a\"}\n"
    );
    assert!(!kept.exists());
    let limits = r#"{"rate_limits":{"five_hour":{"used_percentage":7}}}"#;
    assert_eq!(s.status(limits), format!("{limits}\n"));
    assert!(!kept.exists());
    // Not JSON: nothing to keep, and the user's line still sees it.
    assert_eq!(s.status("not json"), "not json\n");
    assert!(!kept.exists());
    // No line of the user's: prints nothing, and is no failure.
    std::fs::remove_file(s.data.join("original.sh")).unwrap();
    assert_eq!(s.status(limits), "");
}

// Setting it up.
#[test]
fn setup_wraps_the_line_and_teardown_gives_it_back() {
    let s = Sandbox::new("wrap");
    let line = json!({"type": "command", "command": "echo mine", "padding": 2});
    let before = json!({"model": "x", "statusLine": line, "env": {"KEY": "sk-test"}});
    std::fs::write(s.settings(), before.to_string()).unwrap();
    std::fs::set_permissions(s.settings(), std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(s.run_ok("setup").contains("接上了"));
    let now = s.read_settings();
    let command = now["statusLine"]["command"].as_str().unwrap();
    assert!(
        command.starts_with("MOORING_PLUGIN=claude-quota "),
        "{command}"
    );
    assert!(command.contains(" claude-quota status "), "{command}");
    // What else it had stays, its key order and permissions too.
    assert_eq!(now["statusLine"]["padding"], 2);
    assert_eq!(now["env"]["KEY"], "sk-test");
    assert_eq!(mode(&s.settings()), 0o600);
    let keys: Vec<_> = now.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["model", "statusLine", "env"]);
    // The backup may hold keys.
    let backup = s.data.join("settings.before.json");
    assert_eq!(mode(&backup), 0o600);
    assert!(std::fs::read_to_string(&backup)
        .unwrap()
        .contains("sk-test"));
    // Claude Code really runs it, and the user's own line prints.
    let shell = Command::new("/bin/sh")
        .args(["-c", command])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&shell.stdout),
        "mine\n",
        "{shell:?}"
    );
    // Again: nothing to do, and an old readable backup is made private.
    std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(s.run_ok("setup").contains("没有改动"));
    assert_eq!(mode(&backup), 0o600);
    assert!(s.run_ok("teardown").contains("还原"));
    assert_eq!(s.read_settings(), before);
    assert!(!s.data.join("original.json").exists() && !s.data.join("original.sh").exists());
    assert!(s.run_ok("teardown").contains("没有改"));
}

#[test]
fn with_no_line_before_teardown_leaves_none_and_a_link_stays_a_link() {
    let s = Sandbox::new("noline");
    let real = s.root.join("dotfiles.json");
    std::fs::write(&real, r#"{"model":"x"}"#).unwrap();
    std::os::unix::fs::symlink(&real, s.settings()).unwrap();
    s.run_ok("setup");
    assert!(s.settings().is_symlink());
    assert!(std::fs::read_to_string(&real)
        .unwrap()
        .contains("statusLine"));
    assert!(s.run_ok("teardown").contains("去掉了"));
    assert_eq!(s.read_settings(), json!({"model": "x"}));
}

/// An install of 0.1.x: its script is gone once the plugin updates, and what it kept still counts.
#[test]
fn setup_takes_over_from_the_old_script_keeping_the_users_line() {
    let s = Sandbox::new("oldscript");
    let old = format!(
        "MOORING_PLUGIN=claude-quota sh '/gone/plugins/claude-quota/statusline.sh' '{}'",
        s.data.display()
    );
    let before = json!({"statusLine": {"type": "command", "command": old}});
    std::fs::write(s.settings(), before.to_string()).unwrap();
    std::fs::write(s.data.join("original.sh"), "echo mine\n").unwrap();
    let original = json!({"type": "command", "command": "echo mine"});
    std::fs::write(s.data.join("original.json"), original.to_string()).unwrap();
    assert!(s.run_ok("setup").contains("更新"));
    let command = s.read_settings()["statusLine"]["command"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!command.contains("statusline.sh"), "{command}");
    assert_eq!(s.status("{}"), "mine\n");
    s.run_ok("teardown");
    assert_eq!(s.read_settings()["statusLine"], original);
}

#[test]
fn settings_it_cant_read_are_left_alone() {
    let s = Sandbox::new("broken");
    std::fs::write(s.settings(), "{ not json").unwrap();
    assert!(!s.command("setup").output().unwrap().status.success());
    assert_eq!(std::fs::read_to_string(s.settings()).unwrap(), "{ not json");
}

/// The entry names this program by its path; an App moved elsewhere must not leave it dangling.
#[test]
fn the_entry_follows_the_program_only_when_the_one_named_is_gone() {
    let s = Sandbox::new("repair");
    let entry = |program: &str, data: &Path| {
        json!({"statusLine": {"type": "command", "command":
            format!("MOORING_PLUGIN=claude-quota '{program}' claude-quota status '{}'", data.display())}})
    };
    let read = || {
        s.read_settings()["statusLine"]["command"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    std::fs::write(s.settings(), entry("/gone/mooring", &s.data).to_string()).unwrap();
    Sandbox::windows(&s.tick(false));
    assert!(read().contains(BIN), "{}", read());
    // Another build's, still there: not ours to take.
    std::fs::write(s.settings(), entry("/bin/ls", &s.data).to_string()).unwrap();
    Sandbox::windows(&s.tick(false));
    assert!(read().contains("/bin/ls"));
    // Gone, but for another data directory: another install's.
    let other = entry("/gone/mooring", Path::new("/other/data"));
    std::fs::write(s.settings(), other.to_string()).unwrap();
    Sandbox::windows(&s.tick(false));
    assert!(read().contains("/gone/mooring"));
}
