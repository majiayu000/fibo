use fibo_platform::shell::{
    SYSTEM_PATH, interactive, login_env, on_path, past_functions, run_then_stay, sh, stay,
};
use fibo_platform::terminal::{Read, Spec, spawn};
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

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

/// This process's environment with `home` as the account's home: the shell's profile is not
/// this machine's.
fn env_at(home: &Path) -> Vec<(OsString, OsString)> {
    let vars = std::env::vars_os()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("HOME"))
        .chain([("HOME".into(), home.as_os_str().to_owned())]);
    login_env(vars)
}

/// What `argv` prints in a terminal, until `want` shows up or time runs out.
fn screen_until(argv: &[String], dir: &Path, want: &str) -> String {
    let (mut child, master) = spawn(&Spec {
        argv,
        cwd: dir,
        env: env_at(dir),
        cols: 200,
        rows: 30,
    })
    .unwrap();
    let mut reader = master.reader().unwrap();
    let mut text = String::new();
    let mut buf = [0u8; 4096];
    let until = Instant::now() + Duration::from_secs(60);
    while Instant::now() < until && !text.contains(want) {
        match reader.next(&mut buf, Some(Duration::from_millis(200))) {
            Ok(Read::Data(n)) => text.push_str(&String::from_utf8_lossy(&buf[..n])),
            Ok(Read::Quiet) => (),
            Ok(Read::Closed) | Err(_) => break,
        }
    }
    let _ = child.kill();
    text
}

/// Says what it was given, for the test below.
#[test]
#[ignore = "started by the_interactive_shell_runs_a_command_and_takes_text_as_text"]
fn says_its_last_argument() {
    let last = std::env::args().next_back().unwrap_or_default();
    println!("WORD[{}]", hex(&last));
}

fn hex(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn the_interactive_shell_runs_a_command_and_takes_text_as_text() {
    let dir = crate::scratch("shell-run");
    let exe = std::env::current_exe().unwrap();
    let command = format!(
        "{} shell::says_its_last_argument --exact --ignored --nocapture --",
        past_functions(&exe.to_string_lossy())
    );
    let text = "mo'or \"ing\" $HOME ; y";
    let want = format!("WORD[{}]", hex(text));
    let screen = screen_until(
        &run_then_stay(&interactive(), &command, Some(text)),
        &dir,
        &want,
    );
    assert!(screen.contains(&want), "{screen:?}");
}

#[test]
fn the_interactive_shell_stays_after_its_command() {
    let dir = crate::scratch("shell-stays");
    let (mut child, master) = spawn(&Spec {
        argv: &run_then_stay(&interactive(), "echo", Some("up")),
        cwd: &dir,
        env: env_at(&dir),
        cols: 100,
        rows: 30,
    })
    .unwrap();
    let mut reader = master.reader().unwrap();
    let mut buf = [0u8; 4096];
    let until = Instant::now() + Duration::from_secs(5);
    let mut closed = false;
    while Instant::now() < until {
        match reader.next(&mut buf, Some(Duration::from_millis(200))) {
            Ok(Read::Closed) | Err(_) => {
                closed = true;
                break;
            }
            Ok(_) => (),
        }
    }
    let _ = child.kill();
    assert!(!closed, "the shell left after its command");
}

#[test]
fn the_interactive_shell_starts_for_someone_to_work_in() {
    let dir = crate::scratch("shell-start");
    let (mut child, master) = spawn(&Spec {
        argv: &stay(&interactive()),
        cwd: &dir,
        env: env_at(&dir),
        cols: 100,
        rows: 30,
    })
    .unwrap();
    let mut reader = master.reader().unwrap();
    let mut buf = [0u8; 4096];
    let until = Instant::now() + Duration::from_secs(60);
    let mut said = false;
    while Instant::now() < until && !said {
        match reader.next(&mut buf, Some(Duration::from_millis(200))) {
            Ok(Read::Data(n)) => said = n > 0,
            Ok(Read::Quiet) => (),
            Ok(Read::Closed) | Err(_) => break,
        }
    }
    let _ = child.kill();
    assert!(said, "no prompt");
}

#[test]
fn the_login_environment_leaves_out_what_started_this_process() {
    let dir = crate::scratch("shell-login-env");
    let vars = std::env::vars_os().chain([("MOORING_LAUNCHER_ONLY".into(), "1".into())]);
    let env = login_env(vars);
    assert!(!env.iter().any(|(k, _)| k == "MOORING_LAUNCHER_ONLY"));
    let paths: Vec<_> = env
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("PATH"))
        .collect();
    assert_eq!(paths.len(), 1, "{env:?}");
    assert_eq!(paths[0].0, "PATH");
    let added = on_path(&paths[0].1, &dir);
    assert_eq!(std::env::split_paths(&added).last().unwrap(), dir);
}

#[test]
fn a_directory_goes_on_an_empty_path_alone() {
    let dir = crate::scratch("shell-on-path");
    assert_eq!(on_path("".as_ref(), &dir), dir.as_os_str());
}

#[test]
fn a_program_runs_past_a_function_of_its_name() {
    let dir = crate::scratch("shell-past-functions");
    let name = fibo_platform::system::host_name();
    let line = format!("{} ", past_functions("hostname"));
    let screen = screen_until(&run_then_stay(&interactive(), &line, None), &dir, &name);
    assert!(screen.contains(&name), "{screen:?}");
}
