use super::*;
use crate::market::{Source, index_of};
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kl-follow-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
const HOOKS: &str = r#"{"api":1,"hooks":[{"id":"ok","on":["done"],"command":"true"}]}"#;
const ACTIONS: &str =
    r#"{"api":1,"actions":[{"id":"a","title":"A","command":"true","run":"split"}]}"#;
/// The catalog's `h` at `version`, providing hooks (and actions when `more`).
fn offer(catalog: &Path, version: &str, more: bool) -> Offer {
    let d = catalog.join("h");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let provides = if more {
        r#"["hooks","actions"]"#
    } else {
        r#"["hooks"]"#
    };
    std::fs::write(
        d.join("plugin.json"),
        format!(r#"{{"api":1,"id":"h","name":"H","version":"{version}","provides":{provides}}}"#),
    )
    .unwrap();
    std::fs::write(d.join("hooks.json"), HOOKS).unwrap();
    if more {
        std::fs::write(d.join("actions.json"), ACTIONS).unwrap();
    }
    let entry = index_of(catalog).unwrap().plugins.remove(0);
    Offer {
        entry,
        source: Source::Bundled(catalog.to_owned()),
        publisher: Publisher::Official,
    }
}
fn version(plugins: &Path) -> String {
    let p = crate::plugin::check(&plugins.join("h")).unwrap();
    p.version.unwrap()
}
/// `h` 1.0.0 installed from the catalog, with 2.0.0 (or what `next` makes) now shipped.
fn installed_then(name: &str) -> (PathBuf, PathBuf) {
    let (catalog, plugins) = (tmp(name), tmp(&format!("{name}-into")));
    offer(&catalog, "1.0.0", false)
        .install_into(&plugins)
        .unwrap();
    (catalog, plugins)
}

/// Undoing the call in `plugin::install` (or this) leaves swapped Apps on old plugins.
#[test]
fn a_catalog_plugin_follows_the_newer_version_the_app_ships() {
    let (catalog, plugins) = installed_then("follows");
    let done = follow(&plugins, vec![offer(&catalog, "2.0.0", false)]);
    assert_eq!(done, ["h 1.0.0 → 2.0.0"]);
    assert_eq!(version(&plugins), "2.0.0");
    // Run again (the app and the host both start): nothing left to do.
    assert!(follow(&plugins, vec![offer(&catalog, "2.0.0", false)]).is_empty());
    // An older App never takes it back.
    assert!(follow(&plugins, vec![offer(&catalog, "1.0.0", false)]).is_empty());
    assert_eq!(version(&plugins), "2.0.0");
}

#[test]
fn what_would_be_asked_about_or_was_edited_waits_for_settings() {
    // The new version does more: that is asked about, never done at start.
    let (catalog, plugins) = installed_then("asks");
    assert!(follow(&plugins, vec![offer(&catalog, "2.0.0", true)]).is_empty());
    assert_eq!(version(&plugins), "1.0.0");
    // Someone edited a file after installing it.
    let (catalog, plugins) = installed_then("edited");
    let file = std::fs::File::options()
        .write(true)
        .open(plugins.join("h/hooks.json"))
        .unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
        .unwrap();
    assert!(follow(&plugins, vec![offer(&catalog, "2.0.0", false)]).is_empty());
    assert_eq!(version(&plugins), "1.0.0");
    // Copied in by hand: no record of where it came from.
    let (catalog, plugins) = installed_then("copied");
    std::fs::remove_dir_all(crate::setup::data_dir(&plugins, "h")).unwrap();
    assert!(follow(&plugins, vec![offer(&catalog, "2.0.0", false)]).is_empty());
    assert_eq!(version(&plugins), "1.0.0");
}
