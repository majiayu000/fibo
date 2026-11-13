//! Replays every saved screen in `fixtures/screens/<cli>/` through that CLI's bundled rules
//! and the default rules `screen_wait` reads with; `expected.json` beside them names the
//! state, and for a prompt the rule and the question. Mismatches are listed by file name.
use fibo_core::model::Agent;
use fibo_host::{screen_ask, screen_rules};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens")
}

/// Every fixture as (cli directory, file name, screen, expectation).
fn fixtures() -> Vec<(String, String, String, serde_json::Value)> {
    let mut out = vec![];
    let mut dirs: Vec<_> = std::fs::read_dir(root())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let cli = dir.file_name().unwrap().to_string_lossy().into_owned();
        let expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap())
                .unwrap();
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "txt"))
            .collect();
        files.sort();
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        for key in expected.as_object().unwrap().keys() {
            assert!(
                names.contains(key),
                "{cli}/expected.json 列了不存在的 {key}"
            );
        }
        for (file, name) in files.iter().zip(names) {
            let want = expected[&name].clone();
            assert!(!want.is_null(), "{cli}/{name} 没写进 expected.json");
            out.push((
                cli.clone(),
                name,
                std::fs::read_to_string(file).unwrap(),
                want,
            ));
        }
    }
    out
}

fn agent(cli: &str) -> Agent {
    Agent::all()
        .into_iter()
        .find(|a| a.key().to_lowercase() == cli)
        .unwrap_or_else(|| panic!("fixtures/screens/{cli} 不是已注册的 CLI"))
}

#[test]
fn every_saved_screen_reads_as_its_expected_state() {
    let fixtures = fixtures();
    let mut wrong = vec![];
    for (cli, name, screen, want) in &fixtures {
        for (label, rules) in [
            ("own", screen_rules::bundled(agent(cli))),
            ("default", screen_rules::default_rules()),
        ] {
            let e = screen_rules::explain(rules, screen);
            let got = (e.state(), e.rule().map(|r| r.id.as_str()), e.question());
            let expected = (
                want["state"].as_str().unwrap(),
                want["rule"].as_str(),
                want["question"].as_str(),
            );
            if got != expected {
                wrong.push(format!(
                    "{cli}/{name} ({label} rules {}): got {got:?}, want {expected:?}; {}",
                    rules.version,
                    e.fallback()
                ));
            }
        }
        // The function the old tests call answers the same.
        assert_eq!(
            fibo_host::recognition::screen_wait(screen).as_deref(),
            want["question"].as_str(),
            "{cli}/{name}"
        );
    }
    let negatives = fixtures.iter().filter(|f| f.3["state"] == "idle").count();
    println!(
        "{} screens, {} negative, {} mismatches",
        fixtures.len(),
        negatives,
        wrong.len()
    );
    assert!(negatives >= 5);
    assert!(wrong.is_empty(), "mismatches:\n{}", wrong.join("\n"));
}

/// What the phone shows and answers: on each screen with an `ask`, the subject in full, and
/// the key `always` presses. mobile/test/parse.test.ts reads the same expectations.
#[test]
fn every_saved_approval_says_what_it_asks_and_which_key_is_always() {
    let (mut wrong, mut asks) = (vec![], 0);
    for (cli, name, screen, want) in &fixtures() {
        if let Some(ask) = want.get("ask") {
            asks += 1;
            let a = screen_ask::approval_ask(screen).unwrap_or_default();
            let word = |k: &str| ask[k].as_str().unwrap_or("").to_owned();
            let got = (&a.kind, &a.subject, &a.note, &a.detail);
            let expected = (
                &word("kind"),
                &word("subject"),
                &word("note"),
                &word("detail"),
            );
            if got != expected {
                wrong.push(format!("{cli}/{name}: got {got:?}, want {expected:?}"));
            }
            assert!(!a.truncated, "{cli}/{name}");
        }
        if want["state"] == "waiting" {
            let e = screen_rules::explain(screen_rules::bundled(agent(cli)), screen);
            if e.approval().is_some() && want.get("ask").is_some() {
                let key = e.always_key();
                if key.as_deref() != want["always"].as_str() {
                    wrong.push(format!(
                        "{cli}/{name}: always {key:?}, want {}",
                        want["always"]
                    ));
                }
                // The button's words, as `ask` reads the screen: without an agent to go by.
                let label = (screen_rules::explain(screen_rules::default_rules(), screen))
                    .always_option()
                    .map(|o| screen_ask::always_words(&o.text));
                if label.as_deref() != want["always_label"].as_str() {
                    wrong.push(format!(
                        "{cli}/{name}: always label {label:?}, want {}",
                        want["always_label"]
                    ));
                }
                // Never the option that hands over every later prompt.
                let picked = key.and_then(|k| {
                    screen.lines().find(|l| {
                        l.trim_start_matches([' ', '❯', '›'])
                            .starts_with(&format!("{k}. "))
                    })
                });
                assert!(
                    !picked.is_some_and(|l| l.contains("auto mode")),
                    "{cli}/{name} would pick auto mode"
                );
            }
        }
    }
    assert!(asks >= 8, "{asks}");
    assert!(wrong.is_empty(), "mismatches:\n{}", wrong.join("\n"));
}
