//! Checking for and installing a release. Every outside effect goes through `Tools`, so each
//! way a check or an install can fail is tested without a network or a signed build.
use super::{BUNDLE_ID, Release, TEAM_ID, Version, select, sum_for};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleInfo {
    pub id: String,
    pub version: String,
}

/// The outside world an update needs.
pub trait Tools {
    /// A small document, e.g. the release JSON or `SHA256SUMS`.
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String>;
    /// A file, written to `to`.
    fn download(&self, url: &str, to: &Path) -> Result<(), String>;
    /// Unpacks a `ditto -c -k --keepParent` zip into the directory `into`.
    fn unzip(&self, zip: &Path, into: &Path) -> Result<(), String>;
    fn bundle_info(&self, app: &Path) -> Result<BundleInfo, String>;
    /// `codesign --verify --deep --strict`.
    fn verify_signature(&self, app: &Path) -> Result<(), String>;
    /// The signing certificate's Team ID.
    fn team_id(&self, app: &Path) -> Result<String, String>;
    /// Gatekeeper's verdict, `spctl --assess`.
    fn assess(&self, app: &Path) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    Latest(Version),
    Newer(Release),
}

/// Asks `url` for the latest release and compares it with `current`.
pub fn check(url: &str, current: &Version, arch: &str, tools: &dyn Tools) -> Result<Found, String> {
    let bytes = tools.fetch(url)?;
    let release = select(&super::parse(&bytes)?, arch)?;
    Ok(if release.version > *current {
        Found::Newer(release)
    } else {
        Found::Latest(release.version)
    })
}

/// Why an update did not install. The installed app is untouched in every case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Homebrew installed this app; `brew upgrade --cask mooring` updates it.
    Homebrew,
    /// This user cannot replace the bundle where it is; the release page has the download.
    NoPermission(String),
    /// The release has no zip for this Mac, or no `SHA256SUMS`.
    NoBuild(String),
    Download(String),
    /// What was downloaded is not what the release says, or not signed by Mooring's team.
    Verify(String),
    /// Moving the bundles failed; the old one is back where it was.
    Swap(String),
}
impl Failure {
    /// Whether the release page is the way forward: it has the files and installing them by
    /// hand works where this could not.
    pub fn page_helps(&self) -> bool {
        matches!(self, Self::NoPermission(_) | Self::NoBuild(_))
    }
    pub fn message(&self) -> String {
        match self {
            Self::Homebrew => format!(
                "这份 Mooring 由 Homebrew 安装，请运行 {}",
                super::BREW_COMMAND
            ),
            Self::NoPermission(why) => format!("没有权限替换这份 App（{why}），请从发布页下载"),
            Self::NoBuild(why) => why.clone(),
            Self::Download(why) => format!("下载失败：{why}"),
            Self::Verify(why) => format!("校验没通过，没有安装：{why}"),
            Self::Swap(why) => format!("替换 App 失败，已恢复原来的版本：{why}"),
        }
    }
}

/// What a step of `install` is doing, for the status line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Downloading,
    Verifying,
    Swapping,
}

pub struct Install<'a> {
    pub release: &'a Release,
    /// The running app's bundle, replaced in place.
    pub bundle: &'a Path,
    /// Where the download goes; nothing of the old or new bundle is kept there.
    pub cache: &'a Path,
    pub brew_prefixes: &'a [PathBuf],
    /// Accept a build Gatekeeper rejects only for not being notarized (`UNNOTARIZED_ENV`).
    pub allow_unnotarized: bool,
}

/// Downloads, verifies and swaps in `release`. On success the bundle at `bundle` is the new
/// version; the running processes keep the old one's files until they exit.
pub fn install(job: &Install, tools: &dyn Tools, step: &dyn Fn(Step)) -> Result<(), Failure> {
    let bundle = job.bundle;
    if super::homebrew_managed(bundle, job.brew_prefixes) {
        return Err(Failure::Homebrew);
    }
    let version = &job.release.version;
    let (Some(zip), Some(sums)) = (&job.release.zip, &job.release.sums) else {
        return Err(Failure::NoBuild(format!(
            "{version} 没有给这台 Mac（{}）的安装包",
            super::arch()
        )));
    };
    let (parent, name) = match (bundle.parent(), bundle.file_name()) {
        (Some(p), Some(n)) => (p, n.to_string_lossy().into_owned()),
        _ => return Err(Failure::Swap(format!("{} 不是一个 App", bundle.display()))),
    };
    // Extracted beside the bundle, so the swap is a rename on one volume.
    let stage = parent.join(format!(".{name}.update"));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir(&stage).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
            Failure::NoPermission(format!("{}：{e}", parent.display()))
        }
        _ => Failure::Swap(format!("{}：{e}", stage.display())),
    })?;
    let result = replaceable(bundle, &stage)
        .map_err(Failure::NoPermission)
        .and_then(|()| fetch_and_swap(job, zip, sums, &stage, &name, tools, step));
    let _ = std::fs::remove_dir_all(&stage);
    result
}

fn fetch_and_swap(
    job: &Install,
    zip: &super::Asset,
    sums: &super::Asset,
    stage: &Path,
    name: &str,
    tools: &dyn Tools,
    step: &dyn Fn(Step),
) -> Result<(), Failure> {
    step(Step::Downloading);
    let listed = tools.fetch(&sums.url).map_err(Failure::Download)?;
    let expected = sum_for(&String::from_utf8_lossy(&listed), &zip.name)
        .ok_or_else(|| Failure::Verify(format!("SHA256SUMS 里没有 {} 的校验和", zip.name)))?;
    let folder = job.cache.join(job.release.version.to_string());
    std::fs::create_dir_all(&folder)
        .map_err(|e| Failure::Download(format!("{}：{e}", folder.display())))?;
    let file = folder.join(&zip.name);
    let _ = std::fs::remove_file(&file);
    let downloaded = tools.download(&zip.url, &file).map_err(Failure::Download);
    let verified = downloaded.and_then(|()| {
        step(Step::Verifying);
        let actual = sha256_file(&file).map_err(Failure::Download)?;
        if actual != expected {
            return Err(Failure::Verify(format!(
                "{} 的 SHA-256 是 {actual}，SHA256SUMS 写的是 {expected}",
                zip.name
            )));
        }
        tools
            .unzip(&file, stage)
            .map_err(|e| Failure::Verify(format!("解不开 {}：{e}", zip.name)))
    });
    // The zip is not needed past this point, whatever happened.
    let _ = std::fs::remove_dir_all(&folder);
    verified?;
    let app = single_app(stage)?;
    verify_app(&app, &job.release.version, job.allow_unnotarized, tools)?;
    step(Step::Swapping);
    swap(job.bundle, &app, name)
}

/// The one `.app` a release zip holds.
fn single_app(stage: &Path) -> Result<PathBuf, Failure> {
    let entries: Vec<PathBuf> = std::fs::read_dir(stage)
        .map_err(|e| Failure::Verify(e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    match entries.as_slice() {
        [app] if app.extension().is_some_and(|e| e == "app") && app.is_dir() => Ok(app.clone()),
        _ => Err(Failure::Verify("压缩包里应当只有一个 Mooring.app".into())),
    }
}

/// The extracted app is Mooring, the version the release says, signed by Mooring's team and
/// accepted by Gatekeeper. Checked where it will run from, so what is swapped in is exactly
/// what was checked.
pub fn verify_app(
    app: &Path,
    version: &Version,
    allow_unnotarized: bool,
    tools: &dyn Tools,
) -> Result<(), Failure> {
    let info = tools.bundle_info(app).map_err(Failure::Verify)?;
    if info.id != BUNDLE_ID {
        return Err(Failure::Verify(format!(
            "包标识是 {}，不是 {BUNDLE_ID}",
            info.id
        )));
    }
    if Version::parse(&info.version).as_ref() != Some(version) {
        return Err(Failure::Verify(format!(
            "App 自称 {}，发布写的是 {version}",
            info.version
        )));
    }
    tools
        .verify_signature(app)
        .map_err(|e| Failure::Verify(format!("签名无效：{e}")))?;
    let team = tools.team_id(app).map_err(Failure::Verify)?;
    if team != TEAM_ID {
        return Err(Failure::Verify(format!(
            "签名的团队是 {team}，不是 {TEAM_ID}"
        )));
    }
    match tools.assess(app) {
        Ok(()) => Ok(()),
        Err(e) if allow_unnotarized && e.contains("Unnotarized Developer ID") => Ok(()),
        Err(e) => Err(Failure::Verify(format!("Gatekeeper 不放行：{e}"))),
    }
}

/// The bundle is this user's to replace: owned by whoever just made `mine` beside it. One
/// owned by someone else (root, after an installer package) can often still be renamed, but
/// not deleted afterwards, and macOS would ask for an administrator.
fn replaceable(bundle: &Path, mine: &Path) -> Result<(), String> {
    match mooring_platform::app_bundle::same_owner(bundle, mine) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("{} 不属于当前用户", bundle.display())),
        Err(e) => Err(format!("{}：{e}", bundle.display())),
    }
}

/// Swaps `new` in at `bundle` by rename, keeping the old one until the new one is in place.
fn swap(bundle: &Path, new: &Path, name: &str) -> Result<(), Failure> {
    let retired = bundle.with_file_name(format!(".{name}.retired"));
    let _ = std::fs::remove_dir_all(&retired);
    std::fs::rename(bundle, &retired)
        .map_err(|e| Failure::Swap(format!("移开 {}：{e}", bundle.display())))?;
    if let Err(e) = std::fs::rename(new, bundle) {
        let back = std::fs::rename(&retired, bundle);
        return Err(Failure::Swap(match back {
            Ok(()) => format!("放入新版本：{e}"),
            Err(b) => format!("放入新版本：{e}；旧版本留在 {}：{b}", retired.display()),
        }));
    }
    // Running processes keep the old files open past the unlink.
    let _ = std::fs::remove_dir_all(&retired);
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}：{e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
