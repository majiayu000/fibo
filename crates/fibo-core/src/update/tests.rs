use super::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

#[test]
fn versions_compare_as_semver() {
    assert!(v("v0.2.0") > v("0.1.9"));
    assert!(v("0.10.0") > v("0.9.0"));
    assert!(v("1.0.0") > v("1.0.0-rc.2"));
    assert!(v("1.0.0-rc.10") > v("1.0.0-rc.2"));
    assert!(v("1.0.0-beta") > v("1.0.0-alpha.5"));
    assert!(v("1.0.0-alpha.1") > v("1.0.0-alpha"));
    assert_eq!(v("v0.1.0"), v("0.1.0+build.7"));
    assert_eq!(v("0.3.1").to_string(), "0.3.1");
    for bad in [
        "", "v", "1.2", "1.2.3.4", "01.2.3", "1.x.3", "1.2.3-", "+1.2.3", "latest",
    ] {
        assert_eq!(Version::parse(bad), None, "{bad}");
    }
}

const LATEST: &str = r#"{
  "url": "https://api.github.com/repos/majiayu000/fibo/releases/1",
  "html_url": "https://github.com/majiayu000/fibo/releases/tag/v0.2.0",
  "tag_name": "v0.2.0", "name": "Fibo 0.2.0", "draft": false, "prerelease": false,
  "body": "notes", "published_at": "2026-11-20T00:00:00Z",
  "assets": [
    {"name": "Fibo-0.2.0-arm64.dmg", "size": 30000000, "browser_download_url": "https://github.com/majiayu000/fibo/releases/download/v0.2.0/Fibo-0.2.0-arm64.dmg"},
    {"name": "Fibo-0.2.0-arm64.zip", "size": 29000000, "content_type": "application/zip", "browser_download_url": "https://github.com/majiayu000/fibo/releases/download/v0.2.0/Fibo-0.2.0-arm64.zip"},
    {"name": "SHA256SUMS", "size": 200, "browser_download_url": "https://github.com/majiayu000/fibo/releases/download/v0.2.0/SHA256SUMS"}
  ]
}"#;

#[test]
fn the_release_json_gives_the_zip_for_this_arch_and_the_sums() {
    let api = parse(LATEST.as_bytes()).unwrap();
    let r = select(&api, "arm64").unwrap();
    assert_eq!(r.version, v("0.2.0"));
    assert_eq!(
        r.page,
        "https://github.com/majiayu000/fibo/releases/tag/v0.2.0"
    );
    let zip = r.zip.unwrap();
    assert_eq!(zip.name, "Fibo-0.2.0-arm64.zip");
    assert!(zip.url.ends_with("/v0.2.0/Fibo-0.2.0-arm64.zip"));
    assert_eq!(r.sums.unwrap().name, "SHA256SUMS");
    // An Intel Mac finds no zip; the release still names its page.
    let intel = select(&api, "x86_64").unwrap();
    assert_eq!(intel.zip, None);
    assert_eq!(
        arch(),
        if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            std::env::consts::ARCH
        }
    );
}

#[test]
fn drafts_prereleases_and_odd_tags_are_not_releases() {
    for (field, value) in [("prerelease", "true"), ("draft", "true")] {
        let text = LATEST.replace(
            &format!("\"{field}\": false"),
            &format!("\"{field}\": {value}"),
        );
        assert!(
            select(&parse(text.as_bytes()).unwrap(), "arm64").is_err(),
            "{field}"
        );
    }
    let text = LATEST.replace("\"tag_name\": \"v0.2.0\"", "\"tag_name\": \"nightly\"");
    assert!(select(&parse(text.as_bytes()).unwrap(), "arm64").is_err());
    assert!(parse(b"{\"message\": \"Not Found\"}").is_err());
    assert!(parse(b"<html>rate limited</html>").is_err());
}

#[test]
fn sums_are_read_from_sha256sum_lines() {
    let a = "a".repeat(64);
    let b = "B".repeat(64);
    let sums = format!("{a}  Fibo-0.2.0-arm64.dmg\n{b} *Fibo-0.2.0-arm64.zip\n");
    assert_eq!(sum_for(&sums, "Fibo-0.2.0-arm64.zip"), Some("b".repeat(64)));
    assert_eq!(sum_for(&sums, "Fibo-0.2.0-arm64.dmg"), Some(a.clone()));
    assert_eq!(sum_for(&sums, "Fibo-0.2.0-x86_64.zip"), None);
    // Two different sums for one file, or a malformed one, are not trusted.
    let twice = format!("{a}  x.zip\n{}  x.zip\n", "c".repeat(64));
    assert_eq!(sum_for(&twice, "x.zip"), None);
    assert_eq!(sum_for("abc  x.zip", "x.zip"), None);
}

#[test]
fn checking_finds_newer_and_same_and_older() {
    let tools = Fake::new(LATEST);
    let url = latest_url(API_BASE);
    assert_eq!(
        url,
        "https://api.github.com/repos/majiayu000/fibo/releases/latest"
    );
    assert!(
        matches!(check(&url, &v("0.1.0"), "arm64", &tools), Ok(Found::Newer(r)) if r.version == v("0.2.0"))
    );
    assert_eq!(
        check(&url, &v("0.2.0"), "arm64", &tools),
        Ok(Found::Latest(v("0.2.0")))
    );
    // A build newer than the release (one packaged from main) is not downgraded.
    assert_eq!(
        check(&url, &v("0.3.0"), "arm64", &tools),
        Ok(Found::Latest(v("0.2.0")))
    );
    let offline = Fake {
        fail_fetch: true,
        ..Fake::new(LATEST)
    };
    assert!(check(&url, &v("0.1.0"), "arm64", &offline).is_err());
}

/// The guard on dev builds: a binary run from `target/` is no bundle, so nothing checks or
/// installs; only `<X>.app/Contents/MacOS/<exe>` is.
#[test]
fn dev_builds_never_update_themselves() {
    for dev in [
        "/Users/me/mooring/target/debug/fibo-app",
        "/Users/me/mooring/target/release/fibo-app",
        "/Users/me/mooring/target/debug/deps/fibo_app-1234",
        "/Applications/Fibo.app/fibo-app",
        "/x/Contents/MacOS/fibo-app",
        "/x/Fibo.app/Contents/Resources/fibo-app",
    ] {
        assert_eq!(packaged_bundle(Path::new(dev)), None, "{dev}");
    }
    assert_eq!(
        packaged_bundle(Path::new("/Applications/Fibo.app/Contents/MacOS/fibo-app")),
        Some(PathBuf::from("/Applications/Fibo.app"))
    );
    // This test binary itself runs from target/.
    assert_eq!(packaged_bundle(&std::env::current_exe().unwrap()), None);
}

#[test]
fn a_check_is_due_once_a_day() {
    assert!(due(None, 1_000));
    assert!(!due(Some(1_000), 1_000 + CHECK_EVERY - 1));
    assert!(due(Some(1_000), 1_000 + CHECK_EVERY));
    // A clock set back does not silence checks for days.
    assert!(due(Some(1_000_000), 10));
}

#[test]
fn the_switch_is_on_until_turned_off_and_unknown_is_not_on() {
    let dir = scratch("switch");
    let file = dir.join("update-check");
    assert!(auto_check(&file));
    std::fs::write(&file, "off\n").unwrap();
    assert!(!auto_check(&file));
    std::fs::write(&file, "on").unwrap();
    assert!(auto_check(&file));
    // A directory in its place cannot be read: not known to be on.
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(!auto_check(&file));
}

#[test]
fn homebrew_installs_are_left_to_brew() {
    let dir = scratch("brew");
    let prefix = dir.join("homebrew");
    let apps = dir.join("Applications");
    std::fs::create_dir_all(apps.join("Fibo.app")).unwrap();
    let prefixes = vec![prefix.clone()];
    assert!(!homebrew_managed(&apps.join("Fibo.app"), &prefixes));
    std::fs::create_dir_all(prefix.join("Caskroom/fibo/0.2.0")).unwrap();
    assert!(homebrew_managed(&apps.join("Fibo.app"), &prefixes));
    std::fs::remove_dir_all(prefix.join("Caskroom/fibo")).unwrap();
    std::fs::create_dir_all(prefix.join("Caskroom/mooring/0.1.0")).unwrap();
    assert!(
        homebrew_managed(&apps.join("Fibo.app"), &prefixes),
        "the cask before the rename"
    );
    let installed = Installed::new("brew-install");
    let tools = Fake::good(&installed);
    let job = installed.job(&prefixes);
    assert_eq!(install(&job, &tools, &|_| ()), Err(Failure::Homebrew));
    assert!(tools.downloads.borrow().is_empty());
    installed.untouched();
}

#[test]
fn a_good_release_replaces_the_bundle_by_rename() {
    let installed = Installed::new("good");
    let tools = Fake::good(&installed);
    let steps = RefCell::new(Vec::new());
    install(&installed.job(&[]), &tools, &|s| steps.borrow_mut().push(s)).unwrap();
    assert_eq!(
        std::fs::read_to_string(installed.bundle.join("Contents/MacOS/fibo-app")).unwrap(),
        "new"
    );
    assert_eq!(
        *steps.borrow(),
        [Step::Downloading, Step::Verifying, Step::Swapping]
    );
    // Nothing is left beside the bundle or in the cache.
    let mut left: Vec<_> = std::fs::read_dir(&installed.apps)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    left.sort();
    assert_eq!(left, ["Fibo.app"]);
    assert!(!installed.cache.join("0.2.0").exists());
}

/// A release still carrying the identifier from before the rename to Fibo installs: the
/// team id is what says it is ours.
#[test]
fn a_release_with_the_old_bundle_id_still_installs() {
    let installed = Installed::new("old-id");
    let mut tools = Fake::good(&installed);
    tools.info.id = OLD_BUNDLE_ID.into();
    install(&installed.job(&[]), &tools, &|_| ()).unwrap();
    assert_eq!(
        std::fs::read_to_string(installed.bundle.join("Contents/MacOS/fibo-app")).unwrap(),
        "new"
    );
}

#[test]
fn every_failed_check_leaves_the_installed_app_alone() {
    type Spoil = fn(&mut Fake);
    let cases: [(&str, Spoil); 10] = [
        ("download", |f| f.fail_download = true),
        ("sums fetch", |f| f.fail_fetch = true),
        ("sha256", |f| f.zip_bytes = b"tampered".to_vec()),
        ("not listed", |f| {
            f.sums = format!("{}  other.zip\n", "0".repeat(64))
        }),
        ("bundle id", |f| f.info.id = "com.example.other".into()),
        ("version", |f| f.info.version = "0.1.5".into()),
        ("signature", |f| {
            f.signature = Err("a sealed resource is missing or invalid".into())
        }),
        ("team", |f| f.team = "ABCDE12345".into()),
        ("gatekeeper", |f| {
            f.assess = Err("rejected\nsource=no usable signature".into())
        }),
        ("unnotarized", |f| {
            f.assess = Err("rejected\nsource=Unnotarized Developer ID".into())
        }),
    ];
    for (name, spoil) in cases {
        let installed = Installed::new(&format!("bad-{}", name.replace(' ', "-")));
        let mut tools = Fake::good(&installed);
        spoil(&mut tools);
        let err = install(&installed.job(&[]), &tools, &|_| ()).unwrap_err();
        let expected_kind = match name {
            "download" | "sums fetch" => matches!(err, Failure::Download(_)),
            _ => matches!(err, Failure::Verify(_)),
        };
        assert!(expected_kind, "{name}: {err:?}");
        assert!(!err.page_helps(), "{name}");
        installed.untouched();
        let left: Vec<_> = std::fs::read_dir(&installed.apps).unwrap().collect();
        assert_eq!(left.len(), 1, "{name}: something left beside the bundle");
    }
}

#[test]
fn unnotarized_is_accepted_only_when_the_test_switch_allows_it() {
    let installed = Installed::new("unnotarized-ok");
    let mut tools = Fake::good(&installed);
    tools.assess = Err("rejected\nsource=Unnotarized Developer ID".into());
    let mut job = installed.job(&[]);
    job.allow_unnotarized = true;
    install(&job, &tools, &|_| ()).unwrap();
    // Other refusals stay refusals.
    let installed = Installed::new("unnotarized-other");
    let mut tools = Fake::good(&installed);
    tools.assess = Err("rejected\nsource=no usable signature".into());
    let mut job = installed.job(&[]);
    job.allow_unnotarized = true;
    assert!(matches!(
        install(&job, &tools, &|_| ()),
        Err(Failure::Verify(_))
    ));
    installed.untouched();
}

#[test]
fn a_release_without_this_macs_build_sends_to_the_page() {
    let installed = Installed::new("no-build");
    let tools = Fake::good(&installed);
    let mut job = installed.job(&[]);
    let mut release = job.release.clone();
    release.zip = None;
    job.release = &release;
    let err = install(&job, &tools, &|_| ()).unwrap_err();
    assert!(matches!(err, Failure::NoBuild(_)) && err.page_helps());
    installed.untouched();
}

#[cfg(unix)]
#[test]
fn a_folder_this_user_cannot_write_sends_to_the_page() {
    use std::os::unix::fs::PermissionsExt;
    let installed = Installed::new("read-only");
    let tools = Fake::good(&installed);
    std::fs::set_permissions(&installed.apps, std::fs::Permissions::from_mode(0o555)).unwrap();
    let err = install(&installed.job(&[]), &tools, &|_| ()).unwrap_err();
    std::fs::set_permissions(&installed.apps, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(err, Failure::NoPermission(_)) && err.page_helps(),
        "{err:?}"
    );
    assert!(tools.downloads.borrow().is_empty());
    installed.untouched();
}

// ---- fixtures ----

fn scratch(name: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("fibo-update-{}-{n}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An installed old bundle in an Applications-like folder, and the release that replaces it.
struct Installed {
    apps: PathBuf,
    bundle: PathBuf,
    cache: PathBuf,
    release: Release,
}
impl Installed {
    fn new(name: &str) -> Self {
        let dir = scratch(name);
        let apps = dir.join("Applications");
        let bundle = apps.join("Fibo.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join("Contents/MacOS/fibo-app"), "old").unwrap();
        let release = select(&parse(LATEST.as_bytes()).unwrap(), "arm64").unwrap();
        Self {
            apps,
            bundle,
            cache: dir.join("cache"),
            release,
        }
    }
    fn job<'a>(&'a self, prefixes: &'a [PathBuf]) -> Install<'a> {
        Install {
            release: &self.release,
            bundle: &self.bundle,
            cache: &self.cache,
            brew_prefixes: prefixes,
            allow_unnotarized: false,
        }
    }
    fn untouched(&self) {
        assert_eq!(
            std::fs::read_to_string(self.bundle.join("Contents/MacOS/fibo-app")).unwrap(),
            "old"
        );
    }
}

struct Fake {
    release_json: String,
    sums: String,
    zip_bytes: Vec<u8>,
    info: BundleInfo,
    signature: Result<(), String>,
    team: String,
    assess: Result<(), String>,
    fail_fetch: bool,
    fail_download: bool,
    downloads: RefCell<Vec<String>>,
}
impl Fake {
    fn new(json: &str) -> Self {
        Self {
            release_json: json.into(),
            sums: String::new(),
            zip_bytes: b"zip".to_vec(),
            info: BundleInfo {
                id: BUNDLE_ID.into(),
                version: "0.2.0".into(),
            },
            signature: Ok(()),
            team: TEAM_ID.into(),
            assess: Ok(()),
            fail_fetch: false,
            fail_download: false,
            downloads: RefCell::new(Vec::new()),
        }
    }
    /// Everything checks out for `installed`'s release.
    fn good(installed: &Installed) -> Self {
        let zip = b"zip".to_vec();
        let file = scratch("sum").join("z");
        std::fs::write(&file, &zip).unwrap();
        let sum = sha256_file(&file).unwrap();
        let name = &installed.release.zip.as_ref().unwrap().name;
        Self {
            sums: format!("{sum}  {name}\n"),
            zip_bytes: zip,
            ..Self::new(LATEST)
        }
    }
}
impl Tools for Fake {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        if self.fail_fetch {
            return Err("Could not resolve host".into());
        }
        Ok(if url.ends_with("SHA256SUMS") {
            self.sums.clone().into_bytes()
        } else {
            self.release_json.clone().into_bytes()
        })
    }
    fn download(&self, url: &str, to: &Path) -> Result<(), String> {
        self.downloads.borrow_mut().push(url.into());
        if self.fail_download {
            std::fs::write(to, b"partial").unwrap();
            return Err("Operation timed out".into());
        }
        std::fs::write(to, &self.zip_bytes).map_err(|e| e.to_string())
    }
    fn unzip(&self, _zip: &Path, into: &Path) -> Result<(), String> {
        let app = into.join("Fibo.app/Contents/MacOS");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("fibo-app"), "new").unwrap();
        Ok(())
    }
    fn bundle_info(&self, app: &Path) -> Result<BundleInfo, String> {
        assert!(app.ends_with("Fibo.app"));
        Ok(self.info.clone())
    }
    fn verify_signature(&self, _: &Path) -> Result<(), String> {
        self.signature.clone()
    }
    fn team_id(&self, _: &Path) -> Result<String, String> {
        Ok(self.team.clone())
    }
    fn assess(&self, _: &Path) -> Result<(), String> {
        self.assess.clone()
    }
}
