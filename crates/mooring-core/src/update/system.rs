//! `Tools` with what the system ships (`mooring_platform::app_bundle`).
use super::{BundleInfo, TEAM_ID, Tools};
use mooring_platform::app_bundle as sys;
use std::path::Path;

/// The largest release zip accepted.
const MAX_ZIP: u64 = 512 << 20;
/// The largest release JSON or SHA256SUMS accepted.
const MAX_DOC: u64 = 4 << 20;

pub struct System {
    /// `http` is followed too: only for a fake release on this machine (`API_ENV`).
    pub allow_http: bool,
}

impl Tools for System {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        sys::fetch(url, self.allow_http, MAX_DOC)
    }
    fn download(&self, url: &str, to: &Path) -> Result<(), String> {
        sys::download(url, to, self.allow_http, MAX_ZIP)
    }
    fn unzip(&self, zip: &Path, into: &Path) -> Result<(), String> {
        sys::unzip(zip, into)
    }
    fn bundle_info(&self, app: &Path) -> Result<BundleInfo, String> {
        Ok(BundleInfo {
            id: sys::info(app, "CFBundleIdentifier")?,
            version: sys::info(app, "CFBundleShortVersionString")?,
        })
    }
    fn verify_signature(&self, app: &Path) -> Result<(), String> {
        sys::verify_signature(app, TEAM_ID)
    }
    fn team_id(&self, app: &Path) -> Result<String, String> {
        sys::team_id(app)
    }
    fn assess(&self, app: &Path) -> Result<(), String> {
        sys::assess(app)
    }
}
