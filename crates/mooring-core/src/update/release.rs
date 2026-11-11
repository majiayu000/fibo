//! What GitHub says the latest release is, and which of its files this Mac installs.
//!
//! The release contract: tag `vX.Y.Z`; assets `Mooring-X.Y.Z-<arch>.zip` (a `ditto -c -k
//! --keepParent` zip of the signed, notarized, stapled Mooring.app), `Mooring-X.Y.Z-<arch>.dmg`
//! and `SHA256SUMS` (`sha256sum` lines: `<hex>  <file name>`).
use super::version::Version;
use serde::Deserialize;

/// One release as `GET /repos/<owner>/<repo>/releases/latest` returns it. Fields GitHub adds
/// are ignored.
#[derive(Debug, Deserialize)]
pub struct ApiRelease {
    pub tag_name: String,
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<ApiAsset>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ApiAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

/// A release this build could install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The release's page, opened whenever installing here is not possible.
    pub page: String,
    /// The zip for this Mac; none when the release has no build for its architecture.
    pub zip: Option<Asset>,
    pub sums: Option<Asset>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// This Mac's name in the asset names.
pub fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    }
}

pub fn parse(bytes: &[u8]) -> Result<ApiRelease, String> {
    serde_json::from_slice(bytes).map_err(|e| t!("看不懂发布信息：{e}", e))
}

/// The release `api` describes, for a Mac of `arch`. A draft, a pre-release or a tag that
/// is not a version is refused: `/releases/latest` never returns those, so one here means
/// the answer is not what it claims.
pub fn select(api: &ApiRelease, arch: &str) -> Result<Release, String> {
    if api.draft || api.prerelease {
        return Err(t!(
            "{tag_name} 是预发布版本，不自动更新",
            tag_name = api.tag_name
        ));
    }
    let version = Version::parse(&api.tag_name)
        .ok_or_else(|| t!("{tag_name} 不是版本号", tag_name = api.tag_name))?;
    let find = |name: &str| {
        api.assets.iter().find(|a| a.name == name).map(|a| Asset {
            name: a.name.clone(),
            url: a.browser_download_url.clone(),
            size: a.size,
        })
    };
    Ok(Release {
        zip: find(&format!("Fibo-{version}-{arch}.zip")),
        sums: find("SHA256SUMS"),
        version,
        page: api.html_url.clone(),
    })
}

/// The SHA-256 `sums` lists for `file`, lower-case. `sha256sum` marks binary mode with
/// `*` before the name; both forms are read. A file listed twice with different sums is
/// not trusted at all.
pub fn sum_for(sums: &str, file: &str) -> Option<String> {
    let mut found: Option<String> = None;
    for line in sums.lines() {
        let Some((hash, name)) = line.trim().split_once(char::is_whitespace) else {
            continue;
        };
        let name = name.trim_start();
        let name = name.strip_prefix('*').unwrap_or(name);
        if name != file {
            continue;
        }
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let hash = hash.to_ascii_lowercase();
        match &found {
            Some(earlier) if *earlier != hash => return None,
            _ => found = Some(hash),
        }
    }
    found
}
