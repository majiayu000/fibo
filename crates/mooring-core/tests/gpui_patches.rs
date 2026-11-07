//! Every GPUI patch must apply to the pinned source, in bootstrap.sh's order. A patch made
//! against a reformatted local `.deps` passes every local check and then stops CI while
//! it prepares its dependencies (f8be35e did). See docs/performance/index.html.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The patch names from the first `for name in ...; do` of bootstrap.sh.
fn patch_names() -> Vec<String> {
    let bootstrap = std::fs::read_to_string(root().join("bootstrap.sh")).expect("bootstrap.sh");
    let line = bootstrap
        .lines()
        .find_map(|line| line.strip_prefix("for name in "))
        .expect("bootstrap.sh lists its patches");
    let names = line.strip_suffix("; do").expect("for ...; do");
    names.split_whitespace().map(str::to_owned).collect()
}

/// Absolute because `git apply` runs from a temp directory; not `canonicalize`, whose
/// `\\?\` form on Windows is a path git cannot open.
fn patch_path(name: &str) -> PathBuf {
    std::path::absolute(root().join("patches").join(format!("gpui-{name}.patch")))
        .unwrap_or_else(|error| panic!("patch {name}: {error}"))
}

/// `crates/x` for each file a patch touches, so only those crates are exported.
fn touched_crates(patch: &str) -> impl Iterator<Item = String> + '_ {
    patch
        .lines()
        .filter_map(|line| line.strip_prefix("+++ b/"))
        .map(|path| path.split('/').take(2).collect::<Vec<_>>().join("/"))
}

#[test]
fn every_gpui_patch_applies_to_the_pinned_source_in_order() {
    let names = patch_names();
    let patches: Vec<(String, PathBuf)> = names
        .iter()
        .map(|name| (name.clone(), patch_path(name)))
        .collect();
    let crates: BTreeSet<String> = patches
        .iter()
        .flat_map(|(_, path)| {
            let text = std::fs::read_to_string(path).expect("patch");
            touched_crates(&text).collect::<Vec<_>>()
        })
        .collect();

    let dir = std::env::temp_dir().join(format!("mooring-gpui-patches-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    // The commit, not the working tree: `.deps` here already has the patches, and may
    // have been reformatted.
    let mut archive = Command::new("git")
        .arg("-C")
        .arg(root().join(".deps/zed"))
        .args(["archive", "HEAD"])
        .args(&crates)
        .stdout(Stdio::piped())
        .spawn()
        .expect("git archive; run bootstrap.sh");
    let extracted = Command::new("tar")
        .args(["-x", "-C"])
        .arg(&dir)
        .stdin(archive.stdout.take().expect("archive output"))
        .status()
        .expect("tar");
    let archived = archive.wait().expect("git archive");
    assert!(
        archived.success() && extracted.success(),
        "could not export the pinned source; run bootstrap.sh"
    );

    let mut failure = None;
    for (name, path) in &patches {
        let applied = Command::new("git")
            .arg("apply")
            .arg(path)
            .current_dir(&dir)
            .output()
            .expect("git apply");
        if !applied.status.success() {
            failure = Some(format!(
                "gpui-{name}.patch does not apply on top of the ones before it: {}\nGenerate \
                 it from `git -C .deps/zed archive HEAD` plus those patches, not from .deps.",
                String::from_utf8_lossy(&applied.stderr)
            ));
            break;
        }
    }
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    if let Some(failure) = failure {
        panic!("{failure}");
    }
}

#[test]
fn bootstrap_applies_every_patch_in_the_folder() {
    let listed: BTreeSet<String> = patch_names()
        .iter()
        .map(|name| format!("gpui-{name}.patch"))
        .collect();
    let present: BTreeSet<String> = std::fs::read_dir(root().join("patches"))
        .expect("patches")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".patch"))
        .collect();
    assert_eq!(listed, present, "bootstrap.sh and patches/ disagree");
}
