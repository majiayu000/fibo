//! `fibo claude-quota`, the program itself against a fake `claude`: the status line, setting
//! it up over what was there, and when `/usage` is really asked. The words `/usage` prints are
//! tested beside their parser (`usage_text_tests.rs`).
#![cfg(unix)]
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_fibo");
const REAL: &str = "Current session: 2% used · resets 8pm (UTC)\nCurrent week (all models): 19% used\nCurrent week (Fable): 0% used";
const COST_ONLY: &str = "Total cost:            $0.0000";

struct Sandbox {
    root: PathBuf,
    data: PathBuf,
    runs: PathBuf,
}
impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let root = std::env::temp_dir().join(format!("mcq-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (data, bin) = (root.join("data"), root.join("bin"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(root.join("home/.claude/projects/p")).unwrap();
        let runs = root.join("runs");
        // Says what `answer` holds, and keeps its arguments and environment to look at.
        write_script(
            &bin.join("claude"),
            &format!(
                "printf '%s\\n' \"$*\" >>'{r}'\nprintf 'CLAUDECODE=%s\\nUSER=%s\\n' \"$CLAUDECODE\" \"$USER\" >>'{r}'\ncat '{a}'\n",
                r = runs.display(),
                a = root.join("answer").display()
            ),
        );
        let s = Sandbox { root, data, runs };
        s.answer(REAL);
        s
    }
    fn answer(&self, result: &str) {
        std::fs::write(
            self.root.join("answer"),
            json!({"is_error": false, "result": result}).to_string(),
        )
        .unwrap();
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
    fn ran(&self) -> usize {
        std::fs::read_to_string(&self.runs)
            .map_or(0, |s| s.lines().filter(|l| l.contains("/usage")).count())
    }
    /// As if the last read was this long ago, with the wait drawn at 5 minutes.
    fn age(&self, seconds: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let tried = self.data.join("usage.tried");
        std::fs::write(&tried, format!("{}\n300\n", now - seconds)).unwrap();
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        let file = std::fs::File::options().write(true).open(&tried).unwrap();
        file.set_modified(long_ago).unwrap();
    }
    fn use_claude(&self) {
        std::fs::write(self.root.join("home/.claude/projects/p/s.jsonl"), "{}\n").unwrap();
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
fn write_script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
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
fn limits_are_kept_only_when_the_payload_has_them_and_the_line_prints_whatever_it_is() {
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
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), limits);
    assert_eq!(mode(&kept), 0o600);
    // Not JSON: nothing to keep, and the user's line still sees it.
    assert_eq!(s.status("not json"), "not json\n");
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), limits);
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
fn a_keepline_wrapper_is_replaced_by_the_line_it_wrapped() {
    let s = Sandbox::new("keepline");
    let wrapped = json!({"type": "command", "command": "echo wrapped"});
    let dir = s.root.join("home/.keepline");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("claude-statusline-original.json"),
        wrapped.to_string(),
    )
    .unwrap();
    let old =
        json!({"statusLine": {"type": "command", "command": "/x/keepline-claude-statusline"}});
    std::fs::write(s.settings(), old.to_string()).unwrap();
    assert!(s.run_ok("setup").contains("Keepline"));
    assert_eq!(s.status("{}"), "wrapped\n");
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

// When /usage is asked.
#[test]
fn off_runs_nothing_and_shows_nothing() {
    let s = Sandbox::new("off");
    assert_eq!(Sandbox::windows(&s.tick(false)), json!({}));
    assert_eq!(s.ran(), 0);
}

#[test]
fn it_asks_only_when_due_and_only_if_claude_was_used() {
    let s = Sandbox::new("due");
    let first = Sandbox::windows(&s.tick(true));
    assert_eq!(first["week"]["used"], 19.0);
    assert_eq!(first["models"][0]["name"], "Fable");
    assert_eq!(s.ran(), 1);
    // Every minute's tick prints what was kept.
    assert_eq!(Sandbox::windows(&s.tick(true)), first);
    assert_eq!(s.ran(), 1);
    // The wait is up, but nobody used Claude: the limit has not moved.
    s.age(301);
    assert_eq!(Sandbox::windows(&s.tick(true)), first);
    assert_eq!(s.ran(), 1);
    s.use_claude();
    Sandbox::windows(&s.tick(true));
    assert_eq!(s.ran(), 2);
    // Stayed isolated: nothing a session file or a hook would come from, not nested.
    let said = std::fs::read_to_string(&s.runs).unwrap();
    for flag in [
        "--no-session-persistence",
        "--tools  --strict-mcp-config --setting-sources ",
    ] {
        assert!(said.contains(flag), "{flag}: {said}");
    }
    assert!(said.lines().all(|l| l != "CLAUDECODE=1"), "{said}");
    // Run with no USER at all: without it Claude Code reads as signed out.
    assert!(said.lines().all(|l| l != "USER="), "{said}");
}

#[test]
fn near_the_limit_a_minute_is_enough_of_a_wait() {
    let s = Sandbox::new("near");
    s.answer("Current session: 95% used");
    Sandbox::windows(&s.tick(true));
    s.age(61);
    s.use_claude();
    Sandbox::windows(&s.tick(true));
    assert_eq!(s.ran(), 2);
}

/// Not knowing is not having none: the app keeps its last reading while this exits non-zero.
#[test]
fn a_failed_read_keeps_failing_out_loud_until_one_works() {
    let s = Sandbox::new("fail");
    Sandbox::windows(&s.tick(true));
    s.answer("Failed to load usage data");
    s.age(301);
    s.use_claude();
    let bad = s.tick(true);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("Failed to load"));
    // Not due again yet: no new run, and still the failure.
    assert!(!s.tick(true).status.success());
    assert_eq!(s.ran(), 2);
    s.answer(REAL);
    s.age(301);
    s.use_claude();
    assert_eq!(Sandbox::windows(&s.tick(true))["week"]["used"], 19.0);
}

#[test]
fn another_process_mid_read_only_prints_what_was_kept() {
    let s = Sandbox::new("lock");
    std::fs::create_dir(s.data.join("usage.lock")).unwrap();
    assert_eq!(Sandbox::windows(&s.tick(true)), json!({}));
    assert_eq!(s.ran(), 0);
}

#[test]
fn turned_off_it_forgets_and_turned_on_it_reads_at_once() {
    let s = Sandbox::new("toggle");
    Sandbox::windows(&s.tick(true));
    assert_eq!(Sandbox::windows(&s.tick(false)), json!({}));
    assert!(!s.data.join("usage.json").exists());
    Sandbox::windows(&s.tick(true));
    assert_eq!(s.ran(), 2);
}

#[test]
fn signing_out_clears_what_was_shown() {
    let s = Sandbox::new("out");
    Sandbox::windows(&s.tick(true));
    s.answer(COST_ONLY);
    s.age(301);
    s.use_claude();
    let doc = Sandbox::windows(&s.tick(true));
    assert!(doc.get("session").is_none(), "{doc}");
}

/// The app's PATH is only the system's when the login shell was slow to say.
#[test]
fn claude_is_found_where_its_installer_puts_it_when_the_path_lost_it() {
    let s = Sandbox::new("installer");
    let local = s.root.join("home/.local/bin");
    std::fs::create_dir_all(&local).unwrap();
    std::fs::rename(s.root.join("bin/claude"), local.join("claude")).unwrap();
    assert_eq!(Sandbox::windows(&s.tick(true))["week"]["used"], 19.0);
    assert_eq!(s.ran(), 1);
}

#[test]
fn missing_claude_says_so() {
    let s = Sandbox::new("none");
    std::fs::remove_file(s.root.join("bin/claude")).unwrap();
    let out = s.tick(true);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("找不到 claude"));
}
