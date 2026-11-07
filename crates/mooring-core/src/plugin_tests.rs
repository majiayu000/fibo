use super::*;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kl-plugin-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn plugin(root: &Path, dir: &str, manifest: &str, files: &[(&str, &str)]) {
    let d = root.join(dir);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("plugin.json"), manifest).unwrap();
    for (f, body) in files {
        let p = d.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
}

#[test]
fn a_bad_plugin_is_reported_and_the_rest_load() {
    let root = tmp("discover");
    plugin(
        &root,
        "a-aider",
        r#"{"api":1,"id":"aider","name":"Aider","provides":["agent"]}"#,
        &[("agent.json", "{}"), ("agents/extra.json", "{}")],
    );
    plugin(&root, "b-broken", "{not json", &[]);
    plugin(
        &root,
        "c-future",
        r#"{"api":2,"id":"future","name":"F","provides":["agent"]}"#,
        &[("agent.json", "{}")],
    );
    plugin(
        &root,
        "d-empty",
        r#"{"api":1,"id":"empty","name":"E","provides":["agent"]}"#,
        &[],
    );
    plugin(
        &root,
        "e-dup",
        r#"{"api":1,"id":"aider","name":"Again","provides":["agent"]}"#,
        &[("agent.json", "{}")],
    );
    std::fs::create_dir_all(root.join("f-nomanifest")).unwrap();
    let found = discover(&root);
    let results: Vec<_> = found
        .iter()
        .map(|f| f.result.as_ref().map(|p| p.id.clone()))
        .collect();
    assert_eq!(results[0], Ok("aider".into()));
    assert!(results[1].as_ref().unwrap_err().contains("格式不对"));
    assert!(results[2].as_ref().unwrap_err().contains("api 2"));
    assert!(results[3].as_ref().unwrap_err().contains("没有它的文件"));
    assert!(results[4].as_ref().unwrap_err().contains("重名"));
    assert!(
        results[5]
            .as_ref()
            .unwrap_err()
            .contains("缺少 plugin.json")
    );
    let aider = found[0].result.as_ref().unwrap();
    let files = aider.files(Kind::Agent);
    assert_eq!(files.len(), 2);
    assert!(files[0].ends_with("agent.json"), "agent.json loads first");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn shipped_example_plugins_load() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let found = discover(&root);
    assert!(found.len() >= 3);
    for f in &found {
        let p = f.result.as_ref().unwrap_or_else(|e| panic!("{e}: {f:?}"));
        let (_, actions) = crate::action::load(p.files(Kind::Actions));
        let (_, rules) = crate::rule::load(p.files(Kind::Rules));
        let (_, sidebar) = crate::sidebar::load(p.files(Kind::Sidebar));
        let (_, hooks) = crate::hook::load(p.files(Kind::Hooks));
        let (_, decorations) = crate::decoration::load(p.files(Kind::Decorations));
        let (_, effects) = crate::effect::load(p.files(Kind::Effects));
        let (_, quota) = crate::quota::load(p.files(Kind::Quota));
        let setup = crate::setup::load(p.files(Kind::Setup));
        for loaded in actions
            .iter()
            .chain(&rules)
            .chain(&sidebar)
            .chain(&hooks)
            .chain(&decorations)
            .chain(&effects)
            .chain(&quota)
            .chain(&setup)
        {
            assert!(loaded.result.is_ok(), "{loaded:?}");
        }
    }
}

#[test]
fn a_turned_off_plugin_is_listed_but_loads_nothing() {
    let root = tmp("switch");
    plugin(
        &root,
        "a-aider",
        r#"{"api":1,"id":"aider","name":"Aider","provides":["agent"]}"#,
        &[("agent.json", "{}")],
    );
    std::fs::write(root.join("disabled.json"), r#"["aider"]"#).unwrap();
    let found = discover(&root);
    assert_eq!(found.len(), 1, "the switch file is not a plugin");
    let p = found[0].result.as_ref().unwrap();
    assert!(!p.enabled);
    assert!(p.files(Kind::Agent).is_empty());
    std::fs::write(root.join("disabled.json"), "not json").unwrap();
    assert!(discover(&root)[0].result.as_ref().unwrap().enabled);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn no_plugins_directory_is_no_plugins() {
    assert!(discover(Path::new("/nonexistent/mooring/plugins")).is_empty());
}

#[test]
fn a_plugin_for_a_newer_mooring_loads_what_this_one_knows() {
    let root = tmp("newer");
    plugin(
        &root,
        "a-mixed",
        r#"{"api":1,"id":"mixed","name":"M","provides":["actions","canvas"],"homepage":"x"}"#,
        &[("actions.json", "{}")],
    );
    plugin(
        &root,
        "b-only-new",
        r#"{"api":1,"id":"only-new","name":"N","provides":["canvas"]}"#,
        &[],
    );
    plugin(
        &root,
        "c-level",
        r#"{"api":1,"id":"level","name":"L","provides":["actions"],"mooring":99}"#,
        &[("actions.json", "{}")],
    );
    plugin(
        &root,
        "d-quota",
        r#"{"api":1,"id":"quota","name":"Q","provides":["quota"]}"#,
        &[("quota.json", "{}")],
    );
    let found = discover(&root);
    let mixed = found[0].result.as_ref().unwrap();
    assert_eq!(mixed.provides, [Kind::Actions]);
    assert_eq!(mixed.warnings.len(), 2, "{:?}", mixed.warnings);
    assert!(mixed.warnings[0].contains("canvas"));
    assert!(mixed.warnings[1].contains("homepage"));
    assert_eq!(mixed.level, 1);
    assert!(
        found[1]
            .result
            .as_ref()
            .unwrap_err()
            .contains("更新 Mooring")
    );
    assert!(
        found[2]
            .result
            .as_ref()
            .unwrap_err()
            .contains("插件级别 99")
    );
    assert_eq!(
        found[3].result.as_ref().unwrap().level,
        2,
        "quota came in level 2"
    );
    let _ = std::fs::remove_dir_all(&root);
}

fn with_setup(root: &Path, id: &str, setup: &str, teardown: &str) {
    plugin(
        root,
        id,
        &format!(r#"{{"api":1,"id":"{id}","name":"x","provides":["setup"],"touches":["~/.x"]}}"#),
        &[(
            "setup.json",
            &format!(r#"{{"api":1,"setup":"{setup}","teardown":"{teardown}"}}"#),
        )],
    );
}

/// A broken teardown must not trap a plugin: asked plainly it stays and says why, forced
/// it goes and says what may be left.
#[test]
fn a_failing_teardown_can_be_overruled() {
    let config = tmp("stuck");
    let plugins = dir(&config);
    std::fs::create_dir_all(&plugins).unwrap();
    with_setup(&plugins, "stuck", "true", "exit 3");
    let err = set_enabled_in(&plugins, "stuck", false, false).unwrap_err();
    assert!(err.contains("退出码 3"), "{err}");
    assert!(!turned_off(&plugins, "stuck"));
    let warned = set_enabled_in(&plugins, "stuck", false, true)
        .unwrap()
        .unwrap();
    assert!(warned.contains("~/.x"), "{warned}");
    assert!(turned_off(&plugins, "stuck"));
    // Back on: its setup runs, and uninstalling meets the same teardown.
    assert_eq!(set_enabled_in(&plugins, "stuck", true, false), Ok(None));
    assert!(
        uninstall_in(&plugins, "stuck", false)
            .unwrap_err()
            .contains("没有卸载")
    );
    assert!(plugins.join("stuck").exists());
    let warned = uninstall_in(&plugins, "stuck", true).unwrap().unwrap();
    assert!(warned.contains("~/.x"), "{warned}");
    assert!(!plugins.join("stuck").exists());
    let _ = std::fs::remove_dir_all(&config);
}

/// A plugin that does not load has nothing else that could ever remove it.
#[test]
fn a_plugin_that_does_not_load_can_be_removed_by_its_directory() {
    let config = tmp("broken-remove");
    let plugins = dir(&config);
    plugin(
        &plugins,
        "broken",
        r#"{"api":1,"id":"broken-id","name":"x","provides":["hooks"],"mooring":99}"#,
        &[("setup.json", "{}")],
    );
    let data = crate::setup::data_dir(&plugins, "broken-id");
    std::fs::create_dir_all(&data).unwrap();
    for bad in ["../broken", "nothing", ".lock"] {
        assert!(uninstall_in(&plugins, bad, false).is_err(), "{bad}");
    }
    let warned = uninstall_in(&plugins, "broken", false).unwrap().unwrap();
    assert!(warned.contains("teardown"), "{warned}");
    assert!(!plugins.join("broken").exists() && !data.exists());
    let _ = std::fs::remove_dir_all(&config);
}

/// A setup that fails on the way on leaves it off and undoes what it did.
#[test]
fn a_setup_that_fails_when_turned_on_leaves_it_off() {
    let config = tmp("on-fails");
    let plugins = dir(&config);
    std::fs::create_dir_all(&plugins).unwrap();
    let mark = config.join("undone");
    with_setup(
        &plugins,
        "p",
        "exit 9",
        &format!("touch {}", crate::plugin_command::sh_path(&mark)),
    );
    std::fs::write(switches(&plugins), r#"["p"]"#).unwrap();
    assert!(
        set_enabled_in(&plugins, "p", true, false)
            .unwrap_err()
            .contains("退出码 9")
    );
    assert!(turned_off(&plugins, "p"));
    assert!(mark.exists());
    let _ = std::fs::remove_dir_all(&config);
}

/// Taking a plugin away takes its secrets out of the keychain too.
#[test]
fn uninstalling_removes_its_secrets() {
    let config = tmp("secret-gone");
    let plugins = dir(&config);
    plugin(
        &plugins,
        "s",
        r#"{"api":1,"id":"s","name":"S","provides":["hooks"],"options":[
          {"key":"token","title":"t","type":"string","default":"","secret":true}]}"#,
        &[(
            "hooks.json",
            r#"{"api":1,"hooks":[{"id":"h","on":["done"],"command":"true"}]}"#,
        )],
    );
    crate::plugin_context::set(&plugins, "s", "token", "abc".into()).unwrap();
    assert_eq!(crate::secret::get("s", "token").as_deref(), Some("abc"));
    assert!(check(&plugins.join("s")).unwrap().level >= 3);
    uninstall_in(&plugins, "s", false).unwrap();
    assert_eq!(crate::secret::get("s", "token"), None);
    let _ = std::fs::remove_dir_all(&config);
}
