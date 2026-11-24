//! Where a package manager owns the install (Linux, `PAGE_ONLY`): which copy is the installed
//! one, and whether the latest release is newer. Nothing is downloaded or replaced; a newer
//! release is offered as a link to its page, which has the packages.
use super::{Found, Release, Version};
use std::path::{Path, PathBuf};

/// Whether an update here is only a notice with the release page.
pub const PAGE_ONLY: bool = fibo_platform::app_bundle::PAGE_ONLY;
/// Whether this app asks for releases at all: it installs them (`SUPPORTED`) or only points
/// at their page (`PAGE_ONLY`).
pub const CHECKS: bool = super::SUPPORTED || PAGE_ONLY;
/// Where the `.deb` puts the app (`packaging/linux/package.py`).
pub const DEB_DIR: &str = "/usr/lib/fibo";

/// The folder the `.deb` installed this program in: `/usr/lib/fibo/bin/<exe>`. A build run
/// from `target/` (or copied anywhere else) is a development build and is never told about
/// releases. `exe` is the resolved path, so `/usr/bin/fibo-app`-style links already point here.
pub fn deb_install_dir(exe: &Path) -> Option<PathBuf> {
    let bin = exe.parent()?;
    let dir = bin.parent()?;
    (bin.file_name()? == "bin" && dir == Path::new(DEB_DIR)).then(|| dir.to_path_buf())
}

/// What a check's answer becomes for this copy (the status bar's phase).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offer {
    Latest(Version),
    /// Only its page: a package manager owns this install (`PAGE_ONLY`).
    Page(Release),
    /// `brew upgrade --cask fibo` installs it.
    Homebrew(Release),
    /// This app downloads and installs it.
    Install(Release),
}

/// `homebrew` is asked only for a newer release this copy could otherwise install itself.
pub fn offer(found: Found, page_only: bool, homebrew: impl FnOnce() -> bool) -> Offer {
    match found {
        Found::Latest(v) => Offer::Latest(v),
        Found::Newer(r) if page_only => Offer::Page(r),
        Found::Newer(r) if homebrew() => Offer::Homebrew(r),
        Found::Newer(r) => Offer::Install(r),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{found, parse, select};
    use super::*;

    fn deb_release() -> Release {
        let json = r#"{
          "html_url": "https://github.com/majiayu000/fibo/releases/tag/v0.1.4",
          "tag_name": "v0.1.4", "draft": false, "prerelease": false,
          "assets": [{"name": "fibo_0.1.4_amd64.deb", "size": 1, "browser_download_url": "https://x/fibo_0.1.4_amd64.deb"}]
        }"#;
        select(&parse(json.as_bytes()).unwrap(), "x86_64").unwrap()
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    /// Only the copy dpkg put in /usr/lib/fibo hears about releases.
    #[test]
    fn the_deb_install_is_known_by_its_folder() {
        assert_eq!(
            deb_install_dir(Path::new("/usr/lib/fibo/bin/fibo-app")),
            Some(PathBuf::from("/usr/lib/fibo"))
        );
        for other in [
            "/home/me/fibo/target/release/fibo-app",
            "/usr/lib/fibo/fibo-app",
            "/usr/lib/fibo/Resources/bin/fibo-app",
            "/opt/fibo/bin/fibo-app",
            "/usr/local/lib/fibo/bin/fibo-app",
            "fibo-app",
        ] {
            assert_eq!(deb_install_dir(Path::new(other)), None, "{other}");
        }
    }

    /// A release with only Linux packages (no zip, no installer for this system) is still
    /// offered when it is newer, with its page; an equal or older one is not.
    #[test]
    fn a_newer_release_is_offered_with_its_page() {
        let release = deb_release();
        assert_eq!((&release.zip, &release.installer), (&None, &None));
        for current in ["0.1.3", "0.1.4-rc.4", "0.0.9"] {
            match found(release.clone(), &v(current)) {
                Found::Newer(r) => assert_eq!(
                    r.page,
                    "https://github.com/majiayu000/fibo/releases/tag/v0.1.4"
                ),
                other => panic!("{current}: {other:?}"),
            }
        }
        for current in ["0.1.4", "0.1.5-rc.1", "0.2.0"] {
            assert_eq!(
                found(release.clone(), &v(current)),
                Found::Latest(v("0.1.4")),
                "{current}"
            );
        }
    }

    /// Where the install is the package manager's, a newer release is only its page, even if
    /// Homebrew would otherwise be asked; elsewhere Homebrew's or this app's to install.
    #[test]
    fn a_newer_release_is_only_its_page_where_a_package_manager_owns_the_install() {
        let r = deb_release();
        let newer = || Found::Newer(r.clone());
        let asked = std::cell::Cell::new(false);
        let homebrew = || {
            asked.set(true);
            true
        };
        assert_eq!(offer(newer(), true, homebrew), Offer::Page(r.clone()));
        assert!(!asked.get(), "Homebrew is not asked about a page-only copy");
        assert_eq!(offer(newer(), false, || true), Offer::Homebrew(r.clone()));
        assert_eq!(offer(newer(), false, || false), Offer::Install(r.clone()));
        for page_only in [true, false] {
            assert_eq!(
                offer(Found::Latest(v("0.1.4")), page_only, || true),
                Offer::Latest(v("0.1.4"))
            );
        }
    }

    /// On Linux the copy checks for releases, keeps the check's time under the user's cache,
    /// and a newer release is offered as its page. Undo `PAGE_ONLY` or `cache_dir` in
    /// `fibo-platform`'s `linux.rs`, or `CHECKS`, and the `.deb` never hears of a release.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_copy_checks_and_is_sent_to_the_release_page() {
        // Compared at run time rather than `assert!` on constants (clippy's
        // `assertions_on_constants`), so undoing one of them fails this test by name.
        assert_eq!(
            (
                PAGE_ONLY,
                CHECKS,
                super::super::SUPPORTED,
                super::super::INSTALLER
            ),
            (true, true, false, false),
            "(PAGE_ONLY, CHECKS, SUPPORTED, INSTALLER) on Linux"
        );
        let cache = super::super::cache_dir().expect("a cache folder on Linux");
        assert!(cache.ends_with("fibo/updates"), "{}", cache.display());
        let r = deb_release();
        assert_eq!(
            offer(Found::Newer(r.clone()), PAGE_ONLY, || false),
            Offer::Page(r)
        );
    }
}
