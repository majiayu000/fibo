use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fibo-cli-en-{}-{}-{}",
            tag,
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        ('\u{3400}'..='\u{4dbf}').contains(&c)
            || ('\u{4e00}'..='\u{9fff}').contains(&c)
            || ('\u{3000}'..='\u{303f}').contains(&c)
            || ('\u{ff00}'..='\u{ffef}').contains(&c)
    })
}

fn run_cli(home: &Path, envs: &[(&str, &str)], args: &[&str]) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mooring"));
    cmd.args(args)
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("MOORING_LANG")
        .env_remove("FIBO_LANG")
        .env_remove("KEEPLINE_LANG");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn mooring");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_renders_english_via_env_and_saved_preference_without_cjk() {
    let tmp = TempDir::new("en");

    for env_pair in [("MOORING_LANG", "en"), ("FIBO_LANG", "en")] {
        for (args, expected) in [
            (&["--help"][..], "List terminals"),
            (&["--skill"][..], "# Controlling Terminals in Fibo"),
            (&["--skill", "plugins"][..], "# Customizing Fibo"),
            (&["--skill", "session"][..], "fibo todo list"),
            (&["plugins", "--help"][..], "Usage: fibo plugins"),
            (&["themes", "names"][..], "bg background (required)"),
        ] {
            let (ok, stdout, stderr) = run_cli(tmp.path(), &[env_pair], args);
            assert!(ok, "{env_pair:?} {args:?} failed: {stderr}");
            assert!(
                stdout.contains(expected),
                "{env_pair:?} {args:?} missing {expected:?}:\n{stdout}"
            );
            assert!(
                !has_cjk(&stdout) && !has_cjk(&stderr),
                "{env_pair:?} {args:?} leaked CJK:\nstdout={stdout}\nstderr={stderr}"
            );
        }

        let (ok, _stdout, stderr) = run_cli(tmp.path(), &[env_pair], &["nonexistent-cmd"]);
        assert!(!ok);
        assert!(
            stderr.contains("unknown command: nonexistent-cmd"),
            "{stderr}"
        );
        assert!(!has_cjk(&stderr), "error leaked CJK:\n{stderr}");
    }

    // Saved `~/.config/fibo/language = en` selects English without env vars.
    let cfg = tmp.path().join(".config/fibo");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(cfg.join("language"), "en\n").unwrap();
    let (ok, stdout, stderr) = run_cli(tmp.path(), &[], &["--help"]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("List terminals"), "{stdout}");
    assert!(!has_cjk(&stdout), "{stdout}");

    // Unbundled binary with no language file or env var stays deterministic in Chinese.
    let fresh_home = TempDir::new("zh");
    let (ok, stdout, _stderr) = run_cli(fresh_home.path(), &[], &["--help"]);
    assert!(ok);
    assert!(stdout.contains("终端列表"), "{stdout}");
}
