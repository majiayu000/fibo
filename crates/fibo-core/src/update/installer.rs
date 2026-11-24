//! Updating where the release ships an installer (Windows): which copy is installed, and
//! fetching the installer the release lists, checked against its `SHA256SUMS`. Running it
//! over the install and starting the new window is `fibo_platform::app_bundle::install_after_exit`.
//!
//! The installer is not signed, so the checksum from the same release over `https` is the
//! whole check: it catches a broken download, not a compromised release.
use super::{Failure, Release, Step, Tools, download_verified};
use std::path::{Path, PathBuf};

/// Whether an update here is the release's installer run over the install.
pub const INSTALLER: bool = fibo_platform::app_bundle::INSTALLER;
/// Whether the running terminals outlive an update: a bundle swap hands them to the new host;
/// the installer stops the host, whose terminals cannot pass on there.
pub const KEEPS_TERMINALS: bool = !INSTALLER;

/// The folder the installer put this program in: `<dir>\bin\<exe>` beside the uninstaller
/// it leaves (`<dir>\unins000.exe`). A build run from `target\` has no uninstaller and is a
/// development build, which never updates itself.
pub fn install_dir(exe: &Path) -> Option<PathBuf> {
    let bin = exe.parent()?;
    let dir = bin.parent()?;
    (bin.file_name()?.eq_ignore_ascii_case("bin") && dir.join("unins000.exe").is_file())
        .then(|| dir.to_path_buf())
}

/// Downloads `release`'s installer into `<cache>/<version>` and checks its SHA-256. The file
/// it returns is left for the installer run to delete; anything that failed is removed.
pub fn download_installer(
    release: &Release,
    cache: &Path,
    tools: &dyn Tools,
    step: &dyn Fn(Step),
) -> Result<PathBuf, Failure> {
    let version = &release.version;
    let (Some(installer), Some(sums)) = (&release.installer, &release.sums) else {
        return Err(Failure::NoBuild(t!(
            "{version} 没有 Windows 安装包",
            version
        )));
    };
    let folder = cache.join(version.to_string());
    let file = download_verified(installer, sums, &folder, tools, step);
    if file.is_err() {
        let _ = std::fs::remove_dir_all(&folder);
    }
    file
}

#[cfg(test)]
mod tests {
    use super::super::{BundleInfo, parse, select, sha256_file};
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fibo-installer-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const RELEASE: &str = r#"{
      "html_url": "https://github.com/majiayu000/fibo/releases/tag/v0.2.0",
      "tag_name": "v0.2.0", "draft": false, "prerelease": false,
      "assets": [
        {"name": "Fibo-0.2.0-arm64.zip", "size": 1, "browser_download_url": "https://x/Fibo-0.2.0-arm64.zip"},
        {"name": "Fibo-Setup-0.2.0.exe", "size": 5, "browser_download_url": "https://x/Fibo-Setup-0.2.0.exe"},
        {"name": "SHA256SUMS", "size": 1, "browser_download_url": "https://x/SHA256SUMS"}
      ]
    }"#;

    struct Fake {
        sums: String,
        bytes: &'static [u8],
    }
    impl Tools for Fake {
        fn fetch(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(self.sums.clone().into_bytes())
        }
        fn download(&self, _: &str, to: &Path) -> Result<(), String> {
            std::fs::write(to, self.bytes).map_err(|e| e.to_string())
        }
        fn unzip(&self, _: &Path, _: &Path) -> Result<(), String> {
            unreachable!("an installer is not unpacked")
        }
        fn bundle_info(&self, _: &Path) -> Result<BundleInfo, String> {
            unreachable!()
        }
        fn verify_signature(&self, _: &Path) -> Result<(), String> {
            unreachable!()
        }
        fn team_id(&self, _: &Path) -> Result<String, String> {
            unreachable!()
        }
        fn assess(&self, _: &Path) -> Result<(), String> {
            unreachable!()
        }
    }

    fn sum_of(bytes: &[u8]) -> String {
        let file = scratch("sum").join("f");
        std::fs::write(&file, bytes).unwrap();
        sha256_file(&file).unwrap()
    }

    /// Only a copy the installer put there updates itself; `target\release` does not.
    #[test]
    fn an_installed_copy_is_known_by_its_uninstaller() {
        let root = scratch("dir");
        let installed = root.join("Fibo");
        std::fs::create_dir_all(installed.join("bin")).unwrap();
        std::fs::write(installed.join("unins000.exe"), "").unwrap();
        let exe = installed.join("bin").join("fibo-app.exe");
        assert_eq!(install_dir(&exe), Some(installed.clone()));
        let built = root.join("target").join("release");
        std::fs::create_dir_all(&built).unwrap();
        assert_eq!(install_dir(&built.join("fibo-app.exe")), None);
        std::fs::remove_file(installed.join("unins000.exe")).unwrap();
        assert_eq!(install_dir(&exe), None);
    }

    #[test]
    fn the_installer_is_downloaded_and_checked() {
        let release = select(&parse(RELEASE.as_bytes()).unwrap(), "x86_64").unwrap();
        assert_eq!(
            release.installer.as_ref().unwrap().name,
            "Fibo-Setup-0.2.0.exe"
        );
        let cache = scratch("ok");
        let good = Fake {
            sums: format!("{}  Fibo-Setup-0.2.0.exe\n", sum_of(b"setup")),
            bytes: b"setup",
        };
        let file = download_installer(&release, &cache, &good, &|_| {}).unwrap();
        assert_eq!(file, cache.join("0.2.0").join("Fibo-Setup-0.2.0.exe"));
        assert_eq!(std::fs::read(&file).unwrap(), b"setup");

        // A download that is not what SHA256SUMS lists is refused and not kept.
        let cache = scratch("bad");
        let bad = Fake {
            bytes: b"other",
            ..good
        };
        let err = download_installer(&release, &cache, &bad, &|_| {}).unwrap_err();
        assert!(matches!(err, Failure::Verify(_)), "{err:?}");
        assert!(!cache.join("0.2.0").exists());
    }

    /// A release with no Windows build sends the user to its page.
    #[test]
    fn a_release_without_an_installer_points_to_its_page() {
        let json = RELEASE.replace("Fibo-Setup-0.2.0.exe", "Fibo-Setup-0.1.9.exe");
        let release = select(&parse(json.as_bytes()).unwrap(), "x86_64").unwrap();
        let fake = Fake {
            sums: String::new(),
            bytes: b"",
        };
        let err = download_installer(&release, &scratch("none"), &fake, &|_| {}).unwrap_err();
        assert!(err.page_helps(), "{err:?}");
    }
}
