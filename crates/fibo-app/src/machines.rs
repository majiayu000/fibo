//! Other machines the window shows beside this one: what `machines.json` names and what the
//! window last knew of each. Their rows, name, link and round trip come from each one's own
//! host, reached natively (`workbench/hosts.rs`, docs/decisions/2026-11-15-one-ui-many-hosts.md
//! step 3).
use fibo_remote::protocol::{Card, Machine, Space};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Each connected machine's last round trip, by its host: what typing there waits for. One
/// owner for it, read by every view that names the machine; gone while it is not connected.
static LATENCY: Mutex<Option<std::collections::HashMap<String, Duration>>> = Mutex::new(None);

/// The last round trip to `host`, while it is connected.
pub fn latency(host: &str) -> Option<Duration> {
    LATENCY.lock().unwrap().as_ref()?.get(host).copied()
}
/// Records `rtt` for `host` (None: not connected); true when it moved enough to show.
pub fn set_latency(host: &str, rtt: Option<Duration>) -> bool {
    let mut all = LATENCY.lock().unwrap();
    let all = all.get_or_insert_with(Default::default);
    let before = match rtt {
        Some(rtt) => all.insert(host.to_owned(), rtt),
        None => all.remove(host),
    };
    moved(before, rtt)
}
/// Worth a redraw: appeared, went, or changed by more than 5 ms and a fifth.
fn moved(before: Option<Duration>, now: Option<Duration>) -> bool {
    match (before, now) {
        (Some(a), Some(b)) => {
            let d = a.abs_diff(b);
            d > Duration::from_millis(5) && d * 5 > a
        }
        (a, b) => a.is_some() != b.is_some(),
    }
}

/// One machine as the user wrote it down, kept in `<config>/machines.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    /// What `ssh` connects to: a name from `~/.ssh/config`, or `user@host`.
    pub host: String,
    /// `fibo` on that machine; unset tries `fibo` on its PATH, then the app bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 先不管这台: it stays connected but asks nothing of you here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quiet: bool,
    /// How its host's socket is reached here.
    #[serde(default, skip_serializing_if = "Via::is_ssh")]
    pub via: Via,
    /// Both ways kept and the faster one taken, `via` first when they are even
    /// (docs/multi-machine/routes.md). A separate field, not a third `via`: an older build
    /// ignores it and keeps to `via`, where an unknown `via` would fail the whole file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto: bool,
}

/// The way to a machine (docs/decisions/2026-11-27-computers-over-relay.md).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    /// `ssh -L` to its socket: over Tailscale, the LAN, whatever `ssh` reaches it by.
    #[default]
    Ssh,
    /// `fibo remote dial` through the relay it was paired on (`fibo remote join`): no ssh.
    Relay,
}

impl Via {
    fn is_ssh(&self) -> bool {
        *self == Via::Ssh
    }
    /// The other way.
    pub fn other(self) -> Via {
        match self {
            Via::Ssh => Via::Relay,
            Via::Relay => Via::Ssh,
        }
    }
    /// How the window names it beside a round trip: `ssh 17 ms`, `中继 139 ms`.
    pub fn word(self) -> &'static str {
        match self {
            Via::Ssh => "ssh",
            Via::Relay => t!("中继"),
        }
    }
}

impl Spec {
    pub fn new(host: String) -> Self {
        Self {
            host,
            path: None,
            quiet: false,
            via: Via::Ssh,
            auto: false,
        }
    }
    /// Shell that sets `$p` to `fibo` on that machine: the path written down, else on its
    /// PATH (by the new name, then the old), else the app bundle in ~/Applications, else
    /// /Applications; Fibo.app, else one installed as Mooring.app.
    pub fn find_program(&self) -> String {
        match &self.path {
            Some(path) => format!("p={}", quote(path)),
            None => [
                "p=$(command -v fibo || command -v mooring)",
                "for d in \"$HOME/Applications\" /Applications; do for a in Fibo Mooring; do [ -x \"$p\" ] || p=\"$d/$a.app/Contents/MacOS/mooring\"; done; done",
            ]
            .join("; "),
        }
    }
    /// `ssh` to this machine running `script` there, with the options every link uses.
    pub fn ssh(&self, script: &str) -> Vec<String> {
        let mut argv = self.ssh_options("-T");
        argv.extend([self.host.clone(), script.to_owned()]);
        argv
    }
    /// `ssh <mode>` and the options every link to this machine uses, before the host.
    pub fn ssh_options(&self, mode: &str) -> Vec<String> {
        [
            "ssh",
            mode,
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=2",
        ]
        .map(str::to_owned)
        .to_vec()
    }
}

use fibo_platform::shell::quote_posix as quote;

/// `host` was paired here through the relay (`fibo remote join`): its link is kept as
/// `fibo-host`'s `relay::computer::link_file` writes it.
pub fn relay_paired(host: &str) -> bool {
    let plain =
        !host.is_empty() && (host.chars()).all(|c| c.is_alphanumeric() || "-_.@".contains(c));
    plain
        && fibo_core::agent::config_dir()
            .is_some_and(|d| d.join("relay-links").join(format!("{host}.link")).is_file())
}

pub fn file() -> Option<PathBuf> {
    fibo_core::agent::config_dir().map(|d| d.join("machines.json"))
}

pub fn load() -> io::Result<Vec<Spec>> {
    load_from(&file().ok_or_else(|| io::Error::other(t!("找不到配置目录")))?)
}

fn load_from(file: &std::path::Path) -> io::Result<Vec<Spec>> {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(io::Error::new(
                e.kind(),
                t!("读不了 {display}：{e}", display = file.display(), e),
            ));
        }
    };
    serde_json::from_slice(&bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            t!("{display} 格式不对：{e}", display = file.display(), e),
        )
    })
}

pub fn save(specs: &[Spec]) -> io::Result<()> {
    save_to(
        &file().ok_or_else(|| io::Error::other(t!("找不到配置目录")))?,
        specs,
    )
}

fn save_to(f: &std::path::Path, specs: &[Spec]) -> io::Result<()> {
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    fibo_platform::fs::write_atomic(
        f,
        &serde_json::to_vec_pretty(specs).map_err(io::Error::other)?,
    )
}

/// `specs` with `host` quiet or not; `None` when it is not among them, so a file that could
/// not be read is never written back empty.
fn with_quiet(mut specs: Vec<Spec>, host: &str, quiet: bool) -> Option<Vec<Spec>> {
    specs.iter_mut().find(|s| s.host == host)?.quiet = quiet;
    Some(specs)
}
/// Sets 先不管 on what the file holds now, leaving everything else in it alone.
pub fn set_quiet(host: &str, quiet: bool) -> io::Result<()> {
    let specs = with_quiet(load()?, host, quiet)
        .ok_or_else(|| io::Error::other(t!("设置里没有 {host} 这台电脑", host)))?;
    save(&specs)
}
/// `next` with what `saved` says about 先不管 and the way there: a list held since before the
/// switch or `fibo remote join` does not undo it.
fn keep_quiet(mut next: Vec<Spec>, saved: &[Spec]) -> Vec<Spec> {
    for spec in &mut next {
        if let Some(s) = saved.iter().find(|s| s.host == spec.host) {
            spec.quiet = s.quiet;
            spec.via = s.via;
            spec.auto = s.auto;
        }
    }
    next
}
/// 路线 in settings: both ways (`None`), or only `via`; everything else the file says stays.
pub fn set_route(host: &str, only: Option<Via>) -> io::Result<()> {
    let mut specs = load()?;
    let spec = (specs.iter_mut().find(|s| s.host == host))
        .ok_or_else(|| io::Error::other(t!("设置里没有 {host} 这台电脑", host)))?;
    match only {
        Some(via) => (spec.via, spec.auto) = (via, false),
        None => spec.auto = true,
    }
    save(&specs)
}
/// Saves the list settings edits, which adds and removes machines and nothing else.
pub fn save_keeping_quiet(next: &[Spec]) -> io::Result<()> {
    save_keeping_quiet_to(
        &file().ok_or_else(|| io::Error::other(t!("找不到配置目录")))?,
        next,
    )
}

fn save_keeping_quiet_to(file: &std::path::Path, next: &[Spec]) -> io::Result<()> {
    save_to(file, &keep_quiet(next.to_vec(), &load_from(file)?))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Connecting,
    Online,
    /// Why it is not connected; it keeps retrying.
    Offline(String),
}

/// What the window draws for one machine.
#[derive(Debug, Clone)]
pub struct View {
    pub spec: Spec,
    pub link: Link,
    /// Its own name and id, once it said hello.
    pub machine: Option<Machine>,
    pub spaces: Vec<Space>,
    pub terminals: Vec<Card>,
    /// Its terminals in 置顶, as its host keeps them; None from a host before it did.
    pub pins: Option<Vec<String>>,
}

/// One machine as the window knows it, fed by its own host here (`hosts`).
pub struct Remote {
    pub view: Arc<Mutex<View>>,
}

impl Remote {
    /// A machine not reached yet.
    pub fn new(spec: Spec) -> Self {
        let view = View {
            spec,
            link: Link::Connecting,
            machine: None,
            spaces: Vec::new(),
            terminals: Vec::new(),
            pins: None,
        };
        Self {
            view: Arc::new(Mutex::new(view)),
        }
    }

    /// A machine that lists `terminals` and never connects: for screenshots only.
    #[cfg(feature = "visual-test")]
    pub fn fixed(spec: Spec, spaces: Vec<Space>, terminals: Vec<Card>) -> Self {
        let view = View {
            spec,
            link: Link::Online,
            machine: None,
            spaces,
            terminals,
            pins: None,
        };
        Self {
            view: Arc::new(Mutex::new(view)),
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_round_trip_redraws_only_when_it_moves_enough() {
        let ms = |n| Some(Duration::from_millis(n));
        assert!(moved(None, ms(20)), "appeared");
        assert!(moved(ms(20), None), "went");
        assert!(!moved(ms(20), ms(24)), "jitter under 5 ms");
        assert!(!moved(ms(200), ms(220)), "a tenth");
        assert!(moved(ms(20), ms(40)));
        assert!(moved(ms(200), ms(260)));
    }
    use super::*;

    #[test]
    fn the_program_there_is_found_through_ssh() {
        let named = Spec {
            host: "mac2".into(),
            path: Some("/opt/My Apps/mooring".into()),
            quiet: false,
            via: Via::Ssh,
            auto: false,
        };
        let argv = named.ssh(&named.find_program());
        assert_eq!(argv[0], "ssh");
        assert!(argv.contains(&"BatchMode=yes".to_owned()));
        assert_eq!(argv[argv.len() - 2], "mac2");
        assert_eq!(argv.last().unwrap(), "p='/opt/My Apps/mooring'");
        let unnamed = Spec {
            host: "mac2".into(),
            path: None,
            quiet: false,
            via: Via::Ssh,
            auto: false,
        };
        let probe = unnamed.find_program();
        assert!(probe.contains("command -v fibo || command -v mooring"));
        assert!(probe.contains("for a in Fibo Mooring"));
    }

    fn spec(host: &str, quiet: bool) -> Spec {
        Spec {
            quiet,
            ..Spec::new(host.to_owned())
        }
    }
    #[test]
    fn a_file_from_before_quiet_reads_as_attending_and_attending_is_not_written() {
        let old: Vec<Spec> = serde_json::from_str(r#"[{"host":"apple"}]"#).unwrap();
        assert!(!old[0].quiet);
        let text = serde_json::to_string(&old).unwrap();
        assert!(!text.contains("quiet"), "{text}");
        let text = serde_json::to_string(&[spec("apple", true)]).unwrap();
        let back: Vec<Spec> = serde_json::from_str(&text).unwrap();
        assert!(back[0].quiet, "先不管 did not survive a save: {text}");
    }
    #[test]
    fn quiet_goes_both_ways_and_touches_one_machine() {
        let specs = vec![spec("apple", false), spec("pear", false)];
        let quiet = with_quiet(specs, "apple", true).unwrap();
        assert!(quiet[0].quiet && !quiet[1].quiet);
        let back = with_quiet(quiet, "apple", false).unwrap();
        assert!(!back[0].quiet && !back[1].quiet);
    }
    #[test]
    fn an_unreadable_file_is_never_written_back_empty() {
        assert_eq!(with_quiet(Vec::new(), "apple", true), None);
    }
    #[test]
    fn settings_saving_its_list_keeps_a_machine_on_the_relay() {
        let saved: Vec<Spec> = serde_json::from_str(r#"[{"host":"apple","via":"relay"}]"#).unwrap();
        assert_eq!(saved[0].via, Via::Relay);
        let next = keep_quiet(vec![spec("apple", false), spec("mac2", false)], &saved);
        assert_eq!(next[0].via, Via::Relay);
        assert_eq!(next[1].via, Via::Ssh);
        let text = serde_json::to_string(&next).unwrap();
        assert_eq!(text.matches("via").count(), 1, "ssh is not written: {text}");
    }
    #[test]
    fn both_ways_are_a_field_an_older_build_skips_and_settings_keep() {
        let text = r#"[{"host":"apple","auto":true}]"#;
        // What a build from before `auto` reads: the same fields, no `auto`.
        #[derive(serde::Deserialize)]
        struct Older {
            host: String,
            #[serde(default)]
            via: Via,
        }
        let older: Vec<Older> = serde_json::from_str(text).unwrap();
        assert_eq!((older[0].host.as_str(), older[0].via), ("apple", Via::Ssh));
        let saved: Vec<Spec> = serde_json::from_str(text).unwrap();
        assert!(saved[0].auto);
        let next = keep_quiet(vec![spec("apple", false), spec("mac2", false)], &saved);
        assert!(next[0].auto && !next[1].auto);
        let written = serde_json::to_string(&next).unwrap();
        assert_eq!(
            written.matches("auto").count(),
            1,
            "off is not written: {written}"
        );
    }
    #[test]
    fn settings_adding_a_machine_does_not_undo_quiet() {
        let stale = vec![spec("apple", false), spec("pear", false)];
        let saved = [spec("apple", true)];
        let next = keep_quiet(stale, &saved);
        assert!(next[0].quiet, "the list settings held undid 先不管");
        assert!(!next[1].quiet);
    }
    #[test]
    fn invalid_machine_config_is_not_overwritten_by_settings() {
        let root = std::env::temp_dir().join(format!(
            "fibo-machines-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("machines.json");
        let invalid = br#"[{"host":"apple"}, broken]"#;
        std::fs::write(&file, invalid).unwrap();
        let next = vec![spec("new-machine", false)];
        assert_eq!(
            load_from(&file).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(save_keeping_quiet_to(&file, &next).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), invalid);
        // An unreadable path is not an initial empty configuration either.
        assert!(load_from(&root).is_err());
        assert!(save_keeping_quiet_to(&root, &next).is_err());
        assert!(root.is_dir());
        // A genuinely absent file remains a supported first-run case.
        let missing = root.join("new/machines.json");
        assert!(load_from(&missing).unwrap().is_empty());
        save_keeping_quiet_to(&missing, &next).unwrap();
        assert_eq!(load_from(&missing).unwrap(), next);
        std::fs::remove_dir_all(root).unwrap();
    }
}
