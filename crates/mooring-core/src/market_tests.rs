use super::*;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kl-market-{name}-{}", std::process::id()));
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
fn plugin(root: &Path, id: &str, version: &str) {
    let d = root.join(id);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("plugin.json"),
        format!(
            r#"{{"api":1,"id":"{id}","name":"{id}","version":"{version}","provides":["effects"]}}"#
        ),
    )
    .unwrap();
    std::fs::write(
        d.join("effects.json"),
        r#"{"api":1,"complete":{"style":"sweep","duration_ms":1200}}"#,
    )
    .unwrap();
    // A build leftover that must not travel.
    std::fs::write(d.join("build.sh"), "echo hi").unwrap();
}

#[test]
fn the_shipped_index_is_current() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let want = render(&index_of(&dir).unwrap());
    let have = std::fs::read_to_string(dir.join("index.json")).unwrap_or_default();
    assert!(
        want == have,
        "plugins/index.json 过期了：运行 cargo run -p mooring-host -- plugins index plugins"
    );
    let signature = std::fs::read(dir.join("index.sig")).unwrap_or_default();
    assert_eq!(
        verify_index(have.as_bytes(), &signature),
        Ok(Publisher::Official),
        "plugins/index.sig 过期了：在有官方私钥的机器上运行 mooring plugins index plugins"
    );
}

#[test]
fn installs_only_listed_files_checked_and_replaces_in_place() {
    let catalog = tmp("catalog");
    plugin(&catalog, "quick-sweep", "1.0.0");
    let index = index_of(&catalog).unwrap();
    assert_eq!(
        index.plugins[0]
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["effects.json", "plugin.json"]
    );
    std::fs::write(catalog.join("index.json"), render(&index)).unwrap();
    let offer = Offer {
        entry: index.plugins[0].clone(),
        source: Source::Bundled(catalog.clone()),
        publisher: Publisher::Official,
    };
    let plugins = tmp("plugins");
    // A hand-placed copy under another directory name is replaced where it is.
    plugin(&plugins, "quick-sweep", "0.9.0");
    std::fs::rename(plugins.join("quick-sweep"), plugins.join("mine")).unwrap();
    let at = offer.install_into(&plugins).unwrap();
    assert_eq!(at, plugins.join("mine"));
    assert!(!at.join("build.sh").exists());
    assert!(plugin::check(&at).unwrap().version.as_deref() == Some("1.0.0"));
    assert_eq!(entries(&plugins), 1, "no staging left behind");

    // A tampered file is refused and the installed copy stays.
    std::fs::write(catalog.join("quick-sweep/effects.json"), "{}").unwrap();
    let err = offer.install_into(&plugins).unwrap_err();
    assert!(err.contains("不一致"), "{err}");
    assert!(plugin::check(&at).is_ok());
    assert_eq!(entries(&plugins), 1);
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
}

#[test]
fn unsafe_entries_are_refused_before_fetching() {
    let file = |path: &str| FileEntry {
        path: path.into(),
        sha256: "0".repeat(64),
        size: 1,
    };
    let entry = |id: &str, files: Vec<FileEntry>| Entry {
        id: id.into(),
        name: "x".into(),
        version: "1".into(),
        description: None,
        provides: vec![Kind::Rules],
        mooring: None,
        touches: vec![],
        network: None,
        files,
        dir: None,
        versions: vec![],
    };
    assert!(
        entry("ok", vec![file("plugin.json"), file("a/b.json")])
            .check()
            .is_ok()
    );
    for bad in ["../x", "/etc/passwd", "a/../../x", ".hidden", "a\\b", ""] {
        assert!(
            entry("ok", vec![file("plugin.json"), file(bad)])
                .check()
                .is_err(),
            "{bad}"
        );
    }
    assert!(entry("../ok", vec![file("plugin.json")]).check().is_err());
    assert!(entry("ok", vec![file("rules.json")]).check().is_err());
}

#[test]
fn versions_order_as_semver_does() {
    assert!(newer("1.10.0", "1.9.2"));
    assert!(newer("v2", "1.99.99"));
    assert!(newer("1.0.1", "1.0"));
    assert!(!newer("1.0", "1.0.0"));
    assert!(
        !newer("1.0.0-beta", "1.0.0"),
        "a pre-release is older than its release"
    );
    assert!(newer("1.0.0", "1.0.0-rc.1"));
    assert!(newer("1.0.0-beta.11", "1.0.0-beta.2"));
    assert!(newer("1.0.0-rc.1", "1.0.0-beta"));
    assert!(newer("1.0.0-alpha.1", "1.0.0-alpha"));
    assert!(
        !newer("1.0.0+build.7", "1.0.0"),
        "build metadata does not count"
    );
    assert!(!newer("1.0.0", "1.0.0"));
}

#[test]
fn remote_offers_replace_bundled_only_when_newer() {
    let offer = |id: &str, v: &str, remote: bool| Offer {
        entry: Entry {
            id: id.into(),
            name: id.into(),
            version: v.into(),
            description: None,
            provides: vec![Kind::Rules],
            mooring: None,
            touches: vec![],
            network: None,
            files: vec![],
            dir: None,
            versions: vec![],
        },
        source: if remote {
            Source::Remote("https://x".into())
        } else {
            Source::Bundled(PathBuf::new())
        },
        publisher: Publisher::Official,
    };
    let merged = merge(
        vec![offer("a", "0.2.0", false), offer("b", "0.1.0", false)],
        vec![
            offer("a", "0.10.0", true),
            offer("b", "0.1.0", true),
            offer("c", "1", true),
        ],
    );
    let got: Vec<_> = merged
        .iter()
        .map(|o| (o.entry.id.as_str(), o.origin()))
        .collect();
    assert_eq!(
        got,
        [("a", "远程目录"), ("b", "随 App 附带"), ("c", "远程目录")]
    );
    assert!(newer("1.10.0", "1.9.2") && !newer("1.0.0", "1.0.0"));
    assert!(runs_commands(&[Kind::Effects, Kind::Hooks]));
    assert!(!runs_commands(&[Kind::Effects, Kind::Sidebar]));
}

#[test]
fn a_file_url_index_installs_like_a_remote_one() {
    let catalog = tmp("remote");
    plugin(&catalog, "quick-sweep", "2.0.0");
    std::fs::write(
        catalog.join("index.json"),
        render(&index_of(&catalog).unwrap()),
    )
    .unwrap();
    let url = format!("file://{}/index.json", catalog.display());
    let offers = fetch_remote(&url).unwrap();
    assert_eq!(offers.len(), 1);
    let plugins = tmp("remote-plugins");
    let at = offers[0].install_into(&plugins).unwrap();
    assert_eq!(at, plugins.join("quick-sweep"));
    assert!(fetch_remote(&format!("file://{}/missing.json", catalog.display())).is_err());
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
}

#[test]
fn an_index_from_a_newer_mooring_still_lists_what_this_one_can_install() {
    let index = parse(
        br#"{"api":1,"signed":"x","plugins":[
          {"id":"a","name":"A","version":"1.0.0","provides":["actions","canvas"],"homepage":"h",
           "files":[{"path":"plugin.json","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1,"mode":"x"}]},
          {"id":"b","name":"B","version":"1.0.0","provides":["quota"],"mooring":99,
           "files":[{"path":"plugin.json","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1}]}]}"#,
    )
    .unwrap();
    assert_eq!(index.plugins[0].provides, [Kind::Actions]);
    assert!(index.plugins[0].check().is_ok());
    assert!(
        index.plugins[1]
            .check()
            .unwrap_err()
            .contains("更新 Mooring")
    );
}

#[test]
fn the_confirmation_says_what_it_changes_and_whether_it_goes_online() {
    let d = disclosure(
        &[Kind::Setup],
        &["~/.claude/settings.json".into()],
        Some(false),
    );
    assert!(
        d.iter().any(|l| l.contains("~/.claude/settings.json")),
        "{d:?}"
    );
    assert_eq!(d.last().unwrap(), "声明不联网");
    assert!(disclosure(&[Kind::Quota], &[], Some(true)).contains(&"会联网".to_owned()));
    // Nothing runs: nothing to say about the network.
    assert_eq!(disclosure(&[Kind::Effects], &[], Some(false)).len(), 1);
}

/// The fallback `<plugins>/<id>` directory may hold a different plugin (a copied
/// example whose id was changed): installing refuses instead of deleting it.
#[test]
fn install_keeps_another_plugin_living_in_the_id_directory() {
    let catalog = tmp("other-catalog");
    plugin(&catalog, "quick-sweep", "1.0.0");
    let offer = Offer {
        entry: index_of(&catalog).unwrap().plugins[0].clone(),
        source: Source::Bundled(catalog.clone()),
        publisher: Publisher::Official,
    };
    let plugins = tmp("other-plugins");
    plugin(&plugins, "other", "0.1.0");
    std::fs::rename(plugins.join("other"), plugins.join("quick-sweep")).unwrap();
    let err = offer.install_into(&plugins).unwrap_err();
    let kept = plugin::check(&plugins.join("quick-sweep")).map(|p| p.id);
    let entries = entries(&plugins);
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(err.contains("占用"), "{err}");
    assert_eq!(kept.as_deref(), Ok("other"));
    assert_eq!(entries, 1, "no staging left behind");
}

/// The confirmation for command-running plugins reads the listing's `provides`;
/// a package whose plugin.json provides more is refused.
#[test]
fn install_refuses_a_package_that_provides_more_than_listed() {
    let catalog = tmp("provides");
    plugin(&catalog, "sneaky", "1.0.0");
    let d = catalog.join("sneaky");
    std::fs::write(
        d.join("plugin.json"),
        r#"{"api":1,"id":"sneaky","name":"sneaky","version":"1.0.0","provides":["effects","hooks"]}"#,
    )
    .unwrap();
    std::fs::write(d.join("hooks.json"), r#"{"api":1,"hooks":[]}"#).unwrap();
    let mut entry = index_of(&catalog).unwrap().plugins[0].clone();
    entry.provides = vec![Kind::Effects];
    let offer = Offer {
        entry,
        source: Source::Bundled(catalog.clone()),
        publisher: Publisher::Official,
    };
    let plugins = tmp("provides-into");
    let result = offer.install_into(&plugins);
    let installed = plugins.join("sneaky").exists();
    let _ = std::fs::remove_dir_all(&catalog);
    let _ = std::fs::remove_dir_all(&plugins);
    assert!(result.is_err_and(|e| e.contains("没有登记")));
    assert!(!installed);
}
