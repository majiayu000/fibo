use super::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn every_plugin_and_file_says_whether_it_takes_effect() {
    let root = std::env::temp_dir().join(format!("kl-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(
        &root.join("good/plugin.json"),
        r#"{"api":1,"id":"good","name":"好的","provides":["actions"]}"#,
    );
    write(
        &root.join("good/actions.json"),
        r#"{"api":1,"actions":[{"id":"st","title":"状态","command":"git -C {cwd} status"}]}"#,
    );
    write(
        &root.join("good/rules.json"),
        r#"{"api":1,"verify":[["make"]]}"#,
    );
    write(
        &root.join("badfile/plugin.json"),
        r#"{"api":1,"id":"badfile","name":"坏文件","provides":["effects"]}"#,
    );
    write(
        &root.join("badfile/effects.json"),
        r#"{"api":1,"complete":{"style":"stamp","duration_ms":99999}}"#,
    );
    write(&root.join("nomanifest/actions.json"), "{}");
    let checked = check(&root, &root);
    let text = render(&root, &checked);
    assert!(text.contains("✓ good「好的」"), "{text}");
    assert!(text.contains("✓ 动作 actions.json"), "{text}");
    assert!(text.contains("! rules.json 不会加载"), "{text}");
    assert!(text.contains("✗ badfile「坏文件」"), "{text}");
    assert!(text.contains("duration_ms 要在"), "{text}");
    assert!(text.contains("✗ nomanifest：缺少 plugin.json"), "{text}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_note_is_written_kept_current_and_never_over_the_users_own() {
    let dir = std::env::temp_dir().join(format!("kl-agents-md-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write_note(&dir).unwrap();
    let path = dir.join("AGENTS.md");
    let ours = std::fs::read_to_string(&path).unwrap();
    assert!(ours.starts_with(MARK) && ours.contains("mooring --skill plugins"));
    std::fs::write(&path, format!("{MARK}\n旧版本")).unwrap();
    write_note(&dir).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        ours,
        "an old note is updated"
    );
    std::fs::write(&path, "# 我自己的说明").unwrap();
    write_note(&dir).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# 我自己的说明");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Every JSON example in the guide is a file the loaders accept as written.
#[test]
fn the_guides_examples_all_load() {
    let guide = include_str!("plugins.md");
    let root = std::env::temp_dir().join(format!("kl-guide-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut n = 0;
    for block in guide.split("```json\n").skip(1) {
        let text = block.split("```").next().unwrap();
        let value: serde_json::Value =
            serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}):\n{text}"));
        let file = [
            "actions",
            "presets",
            "decorations",
            "hooks",
            "verify",
            "complete",
            "teardown",
            "sources",
        ]
        .into_iter()
        .find(|k| value.get(k).is_some())
        .map(|k| match k {
            "presets" => "sidebar",
            "verify" => "rules",
            "complete" => "effects",
            "teardown" => "setup",
            "sources" => "quota",
            other => other,
        });
        let Some(kind) = file else {
            continue; // plugin.json or colors.json: checked below / by the app
        };
        n += 1;
        let dir = root.join(format!("example-{n}"));
        write(
            &dir.join("plugin.json"),
            &format!(r#"{{"api":1,"id":"example-{n}","name":"例子","provides":["{kind}"]}}"#),
        );
        write(&dir.join(format!("{kind}.json")), text);
    }
    assert!(n >= 6, "examples found: {n}");
    for c in check(&root, &root) {
        assert!(c.error.is_none(), "{}: {:?}", c.dir.display(), c.error);
        for f in &c.files {
            assert!(
                f.error.is_none(),
                "{}: {:?}\n{}",
                f.file.display(),
                f.error,
                std::fs::read_to_string(&f.file).unwrap()
            );
        }
    }
    std::fs::remove_dir_all(&root).unwrap();
}

/// An Agent copies the guide's hook example as it stands, and event text comes from the
/// conversation: with a stand-in `osascript`, that text must arrive as an argument and
/// never inside the script it runs.
#[test]
fn the_guides_hook_example_keeps_event_text_out_of_the_script() {
    let guide = include_str!("plugins.md");
    let hooks = guide
        .split("```json\n")
        .skip(1)
        .filter_map(|b| serde_json::from_str::<serde_json::Value>(b.split("```").next()?).ok())
        .find(|v| v.get("hooks").is_some())
        .unwrap();
    let command = hooks["hooks"][0]["command"].as_str().unwrap();
    let bin = std::env::temp_dir().join(format!("kl-guide-osa-{}", std::process::id()));
    write(
        &bin.join("osascript"),
        "#!/bin/sh\nwhile [ $# -gt 0 ]; do [ \"$1\" = -e ] && { shift; printf '%s\\n' \"$1\"; }; shift; done\n",
    );
    let exe = bin.join("osascript");
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&exe)
        .status()
        .unwrap();
    let path = format!("{}:/usr/bin:/bin", bin.display());
    const PAYLOAD: &str = "x\" & (do shell script \"touch /tmp/no\") & \"y";
    let out = std::process::Command::new("/bin/sh")
        .args(["-c", command])
        .env("PATH", path)
        .env("MOORING_DETAIL", PAYLOAD)
        .env("MOORING_TITLE", PAYLOAD)
        .output()
        .unwrap();
    let script = String::from_utf8_lossy(&out.stdout).into_owned();
    std::fs::remove_dir_all(&bin).unwrap();
    assert!(!script.is_empty());
    assert!(!script.contains("do shell script"), "{script}");
}

/// What `mooring plugins` says is what a running Mooring would do: an action id another
/// plugin got first is a failure here too, though each file reads fine alone.
#[test]
fn the_check_sees_what_only_shows_between_two_plugins() {
    let root = std::env::temp_dir().join(format!("kl-between-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for id in ["a", "b"] {
        write(
            &root.join(id).join("plugin.json"),
            &format!(r#"{{"api":1,"id":"{id}","name":"{id}","provides":["actions","hooks"]}}"#),
        );
        write(
            &root.join(id).join("actions.json"),
            r#"{"api":1,"actions":[{"id":"same","title":"t","command":"true"}]}"#,
        );
        write(
            &root.join(id).join("hooks.json"),
            r#"{"api":1,"hooks":[{"id":"notify","on":["done"],"command":"true"}]}"#,
        );
    }
    let checked = check(&root, &root);
    let errors = |c: &Checked| {
        c.files
            .iter()
            .filter_map(|f| f.error.clone())
            .collect::<Vec<_>>()
    };
    assert!(errors(&checked[0]).is_empty(), "{:?}", errors(&checked[0]));
    let second = errors(&checked[1]);
    assert_eq!(
        second.len(),
        1,
        "hooks may share a name, actions may not: {second:?}"
    );
    assert!(second[0].contains("same"), "{second:?}");
    std::fs::remove_dir_all(&root).unwrap();
}

/// `plugins run` finds a declaration across kinds and names what is there when it cannot.
#[test]
fn trying_a_declaration_it_does_not_have_says_what_there_is() {
    let root = std::env::temp_dir().join(format!("kl-try-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let d = root.join("mooring/plugins/x");
    write(
        &d.join("plugin.json"),
        r#"{"api":1,"id":"x","name":"X","provides":["hooks"]}"#,
    );
    write(
        &d.join("hooks.json"),
        r#"{"api":1,"hooks":[{"id":"say","on":["done"],"command":"true"}]}"#,
    );
    // SAFETY: only this test reads XDG_CONFIG_HOME through plugins_dir in this binary's
    // other tests, which pass their directories explicitly.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &root) };
    let err = try_run("x", "nope", None, None, false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("say"), "{err}");
    assert!(try_run("x", "say", Some("nonsense".into()), None, false).is_err());
    assert!(try_run("x", "say", None, None, false).is_ok());
    // One Agent read from a file and from a command: trying it runs the command, whichever
    // is declared first, or the only thing there is to run can't be tried.
    let both = root.join("mooring/plugins/both");
    write(
        &both.join("plugin.json"),
        r#"{"api":1,"id":"both","name":"B","provides":["quota"]}"#,
    );
    let ran = root.join("ran");
    let w = r#""windows":[{"minutes":300,"used":"/u"}]"#;
    write(
        &both.join("quota.json"),
        &format!(
            r#"{{"api":1,"sources":[{{"agent":"a","file":"f.json",{w}}},{{"agent":"a","command":"echo '{{}}'; touch {}",{w}}}]}}"#,
            ran.display()
        ),
    );
    try_run("both", "a", None, None, false).unwrap();
    assert!(ran.exists());
    unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
    std::fs::remove_dir_all(&root).unwrap();
}

/// The plugins shipped in the repo are the guide's examples: they must all check out.
#[test]
fn the_bundled_example_plugins_all_take_effect() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let checked = check(&dir, Path::new("/nonexistent-home"));
    assert!(checked.len() >= 10);
    for c in &checked {
        assert!(c.error.is_none(), "{}: {:?}", c.dir.display(), c.error);
        for f in &c.files {
            assert!(f.error.is_none(), "{}: {:?}", f.file.display(), f.error);
        }
    }
}
