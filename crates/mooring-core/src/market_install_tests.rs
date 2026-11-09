use super::*;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kl-install-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
/// What is in the plugins directory but the lock.
fn entries(plugins: &Path) -> usize {
    std::fs::read_dir(plugins)
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_name() != ".lock")
        .count()
}

/// One catalog plugin of `kinds`, its declaration files as given, ready to offer.
fn offer_of(catalog: &Path, manifest: &str, files: &[(&str, &str)]) -> Offer {
    let id: serde_json::Value = serde_json::from_str(manifest).unwrap();
    let id = id["id"].as_str().unwrap().to_owned();
    let d = catalog.join(&id);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("plugin.json"), manifest).unwrap();
    for (name, body) in files {
        std::fs::write(d.join(name), body).unwrap();
    }
    let entry = index_of(catalog)
        .unwrap()
        .plugins
        .into_iter()
        .find(|e| e.id == id)
        .unwrap();
    Offer {
        entry,
        source: Source::Bundled(catalog.to_owned()),
        publisher: Publisher::Official,
    }
}

/// Hashes all right, contents that do not load: the working version stays.
#[test]
fn a_package_that_does_not_load_leaves_the_installed_version() {
    let catalog = tmp("broken-update");
    let plugins = tmp("broken-update-into");
    const GOOD: &str = r#"{"api":1,"hooks":[{"id":"ok","on":["done"],"command":"true"}]}"#;
    offer_of(
        &catalog,
        r#"{"api":1,"id":"h","name":"H","version":"1.0.0","provides":["hooks"]}"#,
        &[("hooks.json", GOOD)],
    )
    .install_into(&plugins)
    .unwrap();
    let err = offer_of(
        &catalog,
        r#"{"api":1,"id":"h","name":"H","version":"2.0.0","provides":["hooks"]}"#,
        &[(
            "hooks.json",
            r#"{"api":1,"hooks":[{"id":"ok","on":["dne"],"command":"true"}]}"#,
        )],
    )
    .install_into(&plugins)
    .unwrap_err();
    let kept = plugin::check(&plugins.join("h")).unwrap().version;
    let hooks = std::fs::read_to_string(plugins.join("h/hooks.json")).unwrap();
    let entries = entries(&plugins);
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(err.contains("dne"), "{err}");
    assert_eq!(kept.as_deref(), Some("1.0.0"));
    assert_eq!(hooks, GOOD);
    assert_eq!(entries, 1, "no staging left behind");
}

/// The listing says less than the package about what it changes or whether it goes
/// online: what you would be shown is not what it is, so it is refused.
#[test]
fn a_package_may_not_claim_more_than_its_listing() {
    let catalog = tmp("liar");
    let plugins = tmp("liar-into");
    let mut offer = offer_of(
        &catalog,
        r#"{"api":1,"id":"l","name":"L","version":"1.0.0","provides":["effects"],"network":true,"touches":["~/.ssh/config"]}"#,
        &[("effects.json", r#"{"api":1,"complete":{"style":"sweep"}}"#)],
    );
    let honest = offer.clone();
    offer.entry.touches.clear();
    let touches = offer.prepare(&plugins).unwrap_err();
    offer.entry.touches = honest.entry.touches.clone();
    offer.entry.network = Some(false);
    let network = offer.prepare(&plugins).unwrap_err();
    let ok = honest.prepare(&plugins).map(|p| p.disclosure.clone());
    let entries = entries(&plugins);
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(touches.contains("~/.ssh/config"), "{touches}");
    assert!(network.contains("联网"), "{network}");
    assert!(ok.unwrap().iter().any(|l| l.contains("~/.ssh/config")));
    assert_eq!(entries, 0, "a dropped preparation leaves nothing");
}

/// The confirmation reads the package: webhook addresses and commands by name, and a
/// webhook counts as going online whatever the manifest says.
#[test]
fn the_confirmation_names_commands_and_webhooks() {
    let catalog = tmp("names");
    let plugins = tmp("names-into");
    let prepared = offer_of(
        &catalog,
        r#"{"api":1,"id":"n","name":"N","version":"1.0.0","provides":["hooks"]}"#,
        &[(
            "hooks.json",
            r#"{"api":1,"hooks":[{"id":"a","on":["done"],"webhook":"https://example.invalid/x"},{"id":"b","on":["done"],"command":"say hi"}]}"#,
        )],
    )
    .prepare(&plugins)
    .unwrap();
    let (lines, ask) = (prepared.lines(), prepared.must_ask);
    drop(prepared);
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(ask);
    assert!(lines.contains(&"会联网".to_owned()), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("https://example.invalid/x")),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l.contains("say hi")), "{lines:?}");
}

/// An update is asked about exactly when it adds to what runs; animations are not.
#[test]
fn an_update_asks_only_when_it_adds_to_what_runs() {
    let catalog = tmp("adds");
    let plugins = tmp("adds-into");
    const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
    offer_of(
        &catalog,
        r#"{"api":1,"id":"u","name":"U","version":"1.0.0","provides":["effects"]}"#,
        &[("effects.json", EFFECT)],
    )
    .install_into(&plugins)
    .unwrap();
    let same = offer_of(
        &catalog,
        r#"{"api":1,"id":"u","name":"U","version":"1.1.0","provides":["effects"]}"#,
        &[("effects.json", r#"{"api":1,"complete":{"style":"fade"}}"#)],
    )
    .prepare(&plugins)
    .unwrap();
    let (same_asks, same_added) = (same.must_ask, same.added.clone());
    drop(same);
    let more = offer_of(
        &catalog,
        r#"{"api":1,"id":"u","name":"U","version":"2.0.0","provides":["effects","setup"]}"#,
        &[
            ("effects.json", EFFECT),
            (
                "setup.json",
                r#"{"api":1,"setup":"evil on","teardown":"evil off"}"#,
            ),
        ],
    )
    .prepare(&plugins)
    .unwrap();
    let (more_asks, more_added) = (more.must_ask, more.added.clone());
    drop(more);
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(!same_asks, "{same_added:?}");
    assert!(more_asks);
    assert!(
        more_added.iter().any(|l| l.contains("安装步骤")),
        "{more_added:?}"
    );
    assert!(
        more_added.iter().any(|l| l.contains("evil on")),
        "{more_added:?}"
    );
}

/// An update whose setup fails puts the old version back; a first install's stays off.
/// Both undo what the failed setup did, and neither counts as installed.
#[test]
fn a_setup_that_fails_never_counts_as_installed() {
    let catalog = tmp("setup-fails");
    let plugins = tmp("setup-fails-into");
    let done = catalog.join("undone");
    const ONE: &str = r#"{"api":1,"id":"f","name":"F","version":"1.0.0","provides":["setup"]}"#;
    const TWO: &str = r#"{"api":1,"id":"f","name":"F","version":"2.0.0","provides":["setup"]}"#;
    let fails = format!(
        r#"{{"api":1,"setup":"exit 9","teardown":"touch {}"}}"#,
        crate::plugin_command::sh_path(&done)
    );
    let works = r#"{"api":1,"setup":"true","teardown":"true"}"#;
    // First install, failing.
    let err = offer_of(&catalog, ONE, &[("setup.json", &fails)])
        .install_into(&plugins)
        .unwrap_err();
    assert!(
        err.contains("没有启用") && err.contains("退出码 9"),
        "{err}"
    );
    assert!(plugin::turned_off(&plugins, "f"));
    assert!(done.exists(), "its teardown ran");
    // Fixed and turned on; then an update whose setup fails.
    offer_of(&catalog, ONE, &[("setup.json", works)])
        .install_into(&plugins)
        .unwrap();
    plugin::set_enabled_in(&plugins, "f", true, false).unwrap();
    let err = offer_of(&catalog, TWO, &[("setup.json", &fails)])
        .install_into(&plugins)
        .unwrap_err();
    assert!(err.contains("旧版还在"), "{err}");
    let kept = plugin::check(&plugins.join("f")).unwrap();
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert_eq!(kept.version.as_deref(), Some("1.0.0"));
}

/// A version that drops its setup: the old teardown runs before its files are gone, and
/// if it cannot, the update does not happen.
#[test]
fn dropping_setup_in_an_update_undoes_it_first() {
    let catalog = tmp("drops-setup");
    let plugins = tmp("drops-setup-into");
    let wrapped = catalog.join("wrapped");
    const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
    let v1 = |teardown: String| {
        format!(
            r#"{{"api":1,"setup":"touch {}","teardown":"{teardown}"}}"#,
            crate::plugin_command::sh_path(&wrapped)
        )
    };
    const ONE: &str =
        r#"{"api":1,"id":"w","name":"W","version":"1.0.0","provides":["setup","effects"]}"#;
    let two = || {
        offer_of(
            &catalog,
            r#"{"api":1,"id":"w","name":"W","version":"2.0.0","provides":["effects"]}"#,
            &[("effects.json", EFFECT)],
        )
    };
    offer_of(
        &catalog,
        ONE,
        &[
            ("setup.json", &v1("exit 3".into())),
            ("effects.json", EFFECT),
        ],
    )
    .install_into(&plugins)
    .unwrap();
    let err = two().install_into(&plugins).unwrap_err();
    assert!(
        err.contains("没有更新") && err.contains("退出码 3"),
        "{err}"
    );
    assert_eq!(
        plugin::check(&plugins.join("w"))
            .unwrap()
            .version
            .as_deref(),
        Some("1.0.0")
    );
    offer_of(
        &catalog,
        ONE,
        &[
            (
                "setup.json",
                &v1(format!("rm {}", crate::plugin_command::sh_path(&wrapped))),
            ),
            ("effects.json", EFFECT),
        ],
    )
    .install_into(&plugins)
    .unwrap();
    assert!(wrapped.exists());
    two().install_into(&plugins).unwrap();
    let (gone, version) = (
        !wrapped.exists(),
        plugin::check(&plugins.join("w")).unwrap().version,
    );
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(gone, "the old teardown ran");
    assert_eq!(version.as_deref(), Some("2.0.0"));
}

/// Someone's own plugin under the same id, files no package put there, another catalog:
/// each is asked about even though an animation alone would not be.
#[test]
fn replacing_something_of_someone_elses_is_asked_about() {
    let catalog = tmp("replacing");
    let plugins = tmp("replacing-into");
    const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
    // Written by hand, with notes beside it.
    let mine = plugins.join("m");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(
        mine.join("plugin.json"),
        r#"{"api":1,"id":"m","name":"我的","version":"0.1.0","provides":["effects"]}"#,
    )
    .unwrap();
    std::fs::write(mine.join("effects.json"), EFFECT).unwrap();
    std::fs::write(mine.join("NOTES.md"), "notes").unwrap();
    let offer = offer_of(
        &catalog,
        r#"{"api":1,"id":"m","name":"别人的","version":"9.0.0","provides":["effects"]}"#,
        &[("effects.json", EFFECT)],
    );
    let asked = offer.prepare(&plugins).unwrap();
    let lines = asked.lines().join("\n");
    assert!(asked.must_ask);
    assert!(
        lines.contains("NOTES.md") && lines.contains("别人的"),
        "{lines}"
    );
    asked.commit().unwrap();
    // Now recorded as from the official catalog; the same from elsewhere is a change.
    let mut other = offer.clone();
    other.entry.version = "9.1.0".into();
    other.source = Source::Remote("https://other.invalid/p".into());
    other.publisher = Publisher::Unsigned;
    let from = other.replacing_for_test(&plugins);
    let same = offer.prepare(&plugins).unwrap().must_ask;
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(from.iter().any(|l| l.contains("发布者变了")), "{from:?}");
    assert!(
        !same,
        "the same package from the same place needs no question"
    );
}

#[test]
fn what_a_crash_left_is_cleared_and_a_lost_plugin_comes_back() {
    let plugins = tmp("recover");
    // A pid no process has: the highest possible, far above macOS's limit.
    let dead = 99_999_999;
    std::fs::create_dir_all(plugins.join(format!(".installing-a-b-{dead}-0"))).unwrap();
    std::fs::create_dir_all(plugins.join(format!(".replaced-lost-{dead}"))).unwrap();
    std::fs::write(plugins.join(format!(".replaced-lost-{dead}/keep")), "x").unwrap();
    std::fs::create_dir_all(plugins.join(format!(".replaced-here-{dead}"))).unwrap();
    std::fs::create_dir_all(plugins.join("here")).unwrap();
    recover(&plugins);
    let keep = plugins.join("lost/keep").exists();
    let hidden = std::fs::read_dir(&plugins)
        .unwrap()
        .filter(|e| {
            let n = e.as_ref().unwrap().file_name();
            let n = n.to_string_lossy();
            n.starts_with('.') && n != ".lock"
        })
        .count();
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(keep);
    assert_eq!(hidden, 0);
}

impl Offer {
    /// What `prepare` would say about replacing, without fetching a remote's files.
    fn replacing_for_test(&self, plugins: &Path) -> Vec<String> {
        let old = plugin::discover(plugins)
            .into_iter()
            .find_map(|f| f.result.ok().filter(|o| o.id == self.entry.id))
            .unwrap();
        replacing(plugins, &old, &old, &self.entry, self)
    }
}

/// A signature vouches for these exact bytes and names its key; any change breaks it.
#[test]
fn a_catalogs_signature_says_who_and_fails_on_any_change() {
    let (secret, public) = new_key("别人的目录").unwrap();
    let sig = sign_index(b"{\"api\":1}", secret.as_bytes()).unwrap();
    assert_eq!(
        verify_index(b"{\"api\":1}", sig.as_bytes()),
        Ok(Publisher::Signed {
            name: "别人的目录".into(),
            key: public.clone()
        })
    );
    assert!(
        verify_index(b"{\"api\":2}", sig.as_bytes())
            .unwrap_err()
            .contains("改过")
    );
    let (other, _) = new_key("x").unwrap();
    let mut forged: serde_json::Value =
        serde_json::from_str(&sign_index(b"{\"api\":1}", other.as_bytes()).unwrap()).unwrap();
    forged["key"] = public.into();
    assert!(verify_index(b"{\"api\":1}", forged.to_string().as_bytes()).is_err());
    assert!(verify_index(b"x", b"not json").is_err());
}

/// This repository's address takes nothing but this repository's key; elsewhere an
/// unsigned catalog is allowed and a bad signature is not.
#[test]
fn the_official_address_accepts_only_the_official_key() {
    let index = b"{}";
    let (secret, _) = new_key("冒充者").unwrap();
    let sig = sign_index(index, secret.as_bytes()).unwrap();
    assert!(publisher_of(DEFAULT_REMOTE, index, None).is_err());
    assert!(publisher_of(DEFAULT_REMOTE, index, Some(sig.as_bytes())).is_err());
    let elsewhere = "https://example.invalid/index.json";
    assert_eq!(
        publisher_of(elsewhere, index, None),
        Ok(Publisher::Unsigned)
    );
    assert!(matches!(
        publisher_of(elsewhere, index, Some(sig.as_bytes())),
        Ok(Publisher::Signed { .. })
    ));
    assert!(publisher_of(elsewhere, b"changed", Some(sig.as_bytes())).is_err());
}

/// Installed under the official key that has since been retired: the next official version
/// is no change of publisher, while anyone else's still is.
#[test]
fn moving_between_official_keys_is_not_a_new_publisher() {
    let catalog = tmp("official-keys");
    let plugins = tmp("official-keys-into");
    const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
    let offer = offer_of(
        &catalog,
        r#"{"api":1,"id":"k","name":"K","version":"1.0.0","provides":["effects"]}"#,
        &[("effects.json", EFFECT)],
    );
    assert_eq!(offer.publisher, Publisher::Official);
    offer.install_into(&plugins).unwrap();
    let record = crate::setup::data_dir(&plugins, "k").join(install::SOURCE_FILE);
    let mut was: InstalledFrom = serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    was.key = Some(OFFICIAL_KEYS[1].to_owned());
    std::fs::write(&record, serde_json::to_vec(&was).unwrap()).unwrap();
    let official = offer.replacing_for_test(&plugins);
    let mut stranger = offer.clone();
    stranger.publisher = Publisher::Signed {
        name: "别人".into(),
        key: "ab".repeat(32),
    };
    let other = stranger.replacing_for_test(&plugins);
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(official.is_empty(), "{official:?}");
    assert!(other.iter().any(|l| l.contains("发布者变了")), "{other:?}");
}

/// Installed from one key: the next version signed by another, or not signed at all, is
/// asked about even for an animation. The same key from a new address is not.
#[test]
fn a_change_of_publisher_is_asked_about() {
    let catalog = tmp("publisher");
    let plugins = tmp("publisher-into");
    const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
    let signed = |key: &str| Publisher::Signed {
        name: "作者".into(),
        key: key.into(),
    };
    let (key_a, key_b) = ("a".repeat(64), "b".repeat(64));
    let mut offer = offer_of(
        &catalog,
        r#"{"api":1,"id":"s","name":"S","version":"1.0.0","provides":["effects"]}"#,
        &[("effects.json", EFFECT)],
    );
    offer.publisher = signed(&key_a);
    let first = offer.prepare(&plugins).unwrap();
    assert!(first.must_ask, "a key not seen before");
    assert!(first.lines().join("").contains(&key_a[..16]));
    first.commit().unwrap();
    let again = offer.prepare(&plugins).unwrap();
    assert!(!again.must_ask, "{:?}", again.lines());
    drop(again);
    offer.publisher = signed(&key_b);
    let changed = offer.prepare(&plugins).unwrap();
    assert!(changed.must_ask && changed.lines().join("").contains("发布者变了"));
    drop(changed);
    offer.publisher = Publisher::Unsigned;
    let unsigned = offer.prepare(&plugins).unwrap();
    assert!(unsigned.must_ask && unsigned.lines().join("").contains("没有签名"));
    drop(unsigned);
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
}

/// A catalog keeping an older version: a build too old for the newest gets that one, from
/// its own directory, and a named version can be asked for.
#[test]
fn a_build_too_old_for_the_newest_gets_the_newest_it_can_run() {
    let catalog = tmp("versions");
    let plugins = tmp("versions-into");
    const EFFECT: &str = r#"{"api":1,"complete":{"style":"sweep"}}"#;
    let write_plugin = |dir: &Path, version: &str, level: u32| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            format!(
                r#"{{"api":1,"id":"v","name":"V","version":"{version}","provides":["effects"],"mooring":{level}}}"#
            ),
        )
        .unwrap();
        std::fs::write(dir.join("effects.json"), EFFECT).unwrap();
    };
    write_plugin(&catalog.join("v"), "3.0.0", 1);
    write_plugin(&catalog.join("v/versions/2.0.0"), "2.0.0", 1);
    write_plugin(&catalog.join("v/versions/1.0.0"), "1.0.0", 1);
    let mut entry = index_of(&catalog).unwrap().plugins.remove(0);
    assert_eq!(
        entry
            .versions
            .iter()
            .map(|e| e.version.as_str())
            .collect::<Vec<_>>(),
        ["2.0.0", "1.0.0"]
    );
    assert_eq!(entry.versions[0].dir.as_deref(), Some("versions/2.0.0"));
    // What this build would see if the newest needed a later Mooring.
    entry.mooring = Some(plugin::LEVEL + 1);
    let chosen = entry.for_this_build();
    assert_eq!(chosen.version, "2.0.0");
    assert_eq!(chosen.versions.len(), 2);
    let offer = Offer {
        entry: chosen,
        source: Source::Bundled(catalog.clone()),
        publisher: Publisher::Official,
    };
    offer.install_into(&plugins).unwrap();
    assert_eq!(
        plugin::check(&plugins.join("v"))
            .unwrap()
            .version
            .as_deref(),
        Some("2.0.0")
    );
    assert!(
        !plugins.join("v/versions").exists(),
        "older versions do not travel"
    );
    offer.pick("1.0.0").unwrap().install_into(&plugins).unwrap();
    assert_eq!(
        plugin::check(&plugins.join("v"))
            .unwrap()
            .version
            .as_deref(),
        Some("1.0.0")
    );
    assert!(offer.pick("9.9.9").is_none());
    // A newer remote that cannot run here does not push out a bundled one that can.
    let mut remote = offer.clone();
    remote.entry.version = "9.0.0".into();
    remote.entry.mooring = Some(plugin::LEVEL + 1);
    let merged = merge(vec![offer.clone()], vec![remote]);
    assert_eq!(merged[0].entry.version, "2.0.0");
    // None it can run: the newest stays, for check to say so.
    entry
        .versions
        .iter_mut()
        .for_each(|e| e.mooring = Some(plugin::LEVEL + 1));
    assert_eq!(entry.for_this_build().version, "3.0.0");
    assert!(
        entry
            .for_this_build()
            .check()
            .unwrap_err()
            .contains("更新 Fibo")
    );
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
}

#[test]
fn a_version_directory_outside_the_plugin_is_refused() {
    let catalog = tmp("dir-escape");
    let mut offer = offer_of(
        &catalog,
        r#"{"api":1,"id":"e","name":"E","version":"1.0.0","provides":["effects"]}"#,
        &[("effects.json", r#"{"api":1,"complete":{"style":"sweep"}}"#)],
    );
    offer.entry.dir = Some("../../etc".into());
    let err = offer.entry.check().unwrap_err();
    let _ = std::fs::remove_dir_all(&catalog);
    assert!(err.contains("不安全"), "{err}");
}

/// The OpenCode plugin in this repository: its newest version declares what only a level 6
/// build reads, and a build before that gets the version it can load. Dropping `mooring` or
/// `versions/0.1.0` leaves an older build with a plugin that does not take effect.
#[test]
fn a_build_too_old_for_the_opencode_declarations_gets_the_version_before_them() {
    let catalog = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let entry = index_of(&catalog)
        .unwrap()
        .plugins
        .into_iter()
        .find(|e| e.id == "opencode")
        .unwrap();
    assert_eq!(entry.version, "0.2.3");
    assert_eq!(entry.mooring, Some(7));
    assert!(entry.compatible(), "this build reads it");
    assert_eq!(entry.for_this_build().version, "0.2.3");

    let mut older_build = entry;
    older_build.mooring = Some(plugin::LEVEL + 1);
    let chosen = older_build.for_this_build();
    // Level 6 builds read what 0.2.2 declares, level 4 ones 0.2.0; earlier ones get 0.1.0.
    assert_eq!(chosen.version, "0.2.2");
    assert_eq!(chosen.mooring, Some(6));
    assert!(chosen.versions.iter().any(|e| e.version == "0.2.0"));
    let older = chosen
        .versions
        .iter()
        .find(|e| e.version == "0.1.0")
        .unwrap();
    assert_eq!(older.mooring, None);
    let old = std::fs::read_to_string(catalog.join("opencode/versions/0.1.0/agent.json")).unwrap();
    assert!(!old.contains("screen_rules") && !old.contains("interrupt"));
}
