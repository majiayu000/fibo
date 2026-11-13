//! A plugin installed from this repository's catalog follows the App it came with: when a
//! new build ships a newer version, the next start installs it, as clicking 「更新」 would.
//! Without this, swapping the App kept the old plugin; `fibo run opencode` waited for an
//! input box the 0.2.2 plugin could not see, while 0.2.3 sat inside the bundle.
use super::install::SOURCE_FILE;
use super::{Offer, Publisher, newer};
use std::path::Path;

/// Updates the installed plugins this build ships newer versions of; returns what it did,
/// one line each (「opencode 0.2.2 → 0.2.3」). Left for 设置 › 插件, which shows 可更新:
/// one not installed from the catalog, one whose files changed since it was installed,
/// and one whose update would be asked about (more permissions, another publisher, files
/// someone put in its folder).
pub fn follow_bundled(plugins: &Path) -> Vec<String> {
    match super::bundled() {
        Ok(offers) => follow(plugins, offers),
        Err(_) => vec![],
    }
}

pub(super) fn follow(plugins: &Path, offers: Vec<Offer>) -> Vec<String> {
    let installed = crate::plugin::discover(plugins);
    let mut done = vec![];
    for offer in offers {
        if offer.publisher != Publisher::Official || !offer.entry.compatible() {
            continue;
        }
        let Some(have) = (installed.iter())
            .filter_map(|f| f.result.as_ref().ok())
            .find(|p| p.id == offer.entry.id)
        else {
            continue;
        };
        let Some(was) = have.version.clone() else {
            continue;
        };
        if !newer(&offer.entry.version, &was) || !untouched(plugins, &have.id, &have.dir) {
            continue;
        }
        let Ok(prepared) = offer.prepare(plugins) else {
            continue;
        };
        if prepared.must_ask {
            continue;
        }
        if prepared.commit().is_ok() {
            done.push(format!("{} {was} → {}", have.id, offer.entry.version));
        }
    }
    done
}

/// Installed through the catalog (it left a record) and no file in its folder changed
/// since: an edited plugin is somebody's work, never replaced without asking.
fn untouched(plugins: &Path, id: &str, dir: &Path) -> bool {
    let record = crate::setup::data_dir(plugins, id).join(SOURCE_FILE);
    let Ok(installed) = std::fs::metadata(&record).and_then(|m| m.modified()) else {
        return false;
    };
    fn newest(at: &Path) -> Option<std::time::SystemTime> {
        let mut out = None;
        for e in std::fs::read_dir(at).ok()?.flatten() {
            let m = e.metadata().ok()?;
            let t = if m.is_dir() {
                newest(&e.path())?
            } else {
                m.modified().ok()?
            };
            out = out.max(Some(t));
        }
        out
    }
    newest(dir).is_some_and(|t| t <= installed)
}

#[cfg(test)]
#[path = "market_follow_tests.rs"]
mod tests;
