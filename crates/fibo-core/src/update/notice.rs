//! Where a package manager owns the install (Linux, `PAGE_ONLY`): which copy is the installed
//! one, and whether the latest release is newer. Nothing is downloaded or replaced; a newer
//! release is offered as a link to its page, which has the packages.
use std::path::{Path, PathBuf};

/// Whether an update here is only a notice with the release page.
pub const PAGE_ONLY: bool = fibo_platform::app_bundle::PAGE_ONLY;
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

#[cfg(test)]
mod tests {
    use super::super::{Found, Version, found, parse, select};
    use super::*;

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
        let json = r#"{
          "html_url": "https://github.com/majiayu000/fibo/releases/tag/v0.1.4",
          "tag_name": "v0.1.4", "draft": false, "prerelease": false,
          "assets": [{"name": "fibo_0.1.4_amd64.deb", "size": 1, "browser_download_url": "https://x/fibo_0.1.4_amd64.deb"}]
        }"#;
        let release = select(&parse(json.as_bytes()).unwrap(), "x86_64").unwrap();
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
}
