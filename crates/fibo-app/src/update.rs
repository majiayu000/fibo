//! The app's side of updating (`fibo_core::update` has the rules): whether this copy can
//! update itself, the daily check, and installing then starting the new bundle.
//!
//! After the swap this process quits and the new bundle starts with the same arguments, as
//! `lifecycle::restart_host` does. Its `ensure_host` finds the old host running other code and
//! has the new host take every terminal over (`spawn_host(root, true)`), exactly as when a
//! bundle is swapped by hand and the app reopened.
use fibo_core::update::{self as core, Found, Release, Tools, Version};
use gpui::{App, AppContext, Global};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Idle,
    Checking,
    Latest(Version),
    Available(Release),
    Installing(Release),
    /// A check that failed; only a click on 检查更新 says so.
    CheckFailed(String),
    /// An install that did not happen. `page` is set when downloading by hand is the way on.
    Failed {
        release: Release,
        message: String,
        page: bool,
    },
    /// Homebrew installed this copy; `brew upgrade --cask fibo` updates it.
    Homebrew(Release),
}

#[derive(Clone, Debug)]
pub struct Updates {
    /// The bundle this app runs from; none for a development build, which never updates.
    pub bundle: Option<PathBuf>,
    pub current: Version,
    pub auto: bool,
    pub phase: Phase,
}
impl Global for Updates {}

fn system() -> core::System {
    core::System {
        allow_http: !core::api_base().1,
    }
}

impl Updates {
    fn detect() -> Self {
        let built = Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version {
            major: 0,
            minor: 0,
            patch: 0,
            pre: None,
        });
        let packaged = core::SUPPORTED
            .then(|| std::env::current_exe().ok())
            .flatten()
            .and_then(|exe| core::packaged_bundle(&exe))
            .and_then(|bundle| {
                let info = system().bundle_info(&bundle).ok()?;
                [core::BUNDLE_ID, core::OLD_BUNDLE_ID]
                    .contains(&info.id.as_str())
                    .then_some((bundle, info.version))
            });
        let (bundle, current) = match packaged {
            Some((bundle, version)) => (Some(bundle), Version::parse(&version).unwrap_or(built)),
            None => (None, built),
        };
        Self {
            bundle,
            current,
            auto: core::auto_check_file().is_some_and(|f| core::auto_check(&f)),
            phase: Phase::Idle,
        }
    }
    /// What the status bar offers: a release to install, one being installed, or one only
    /// Homebrew or the release page can install.
    pub fn offered(&self) -> Option<&Release> {
        match &self.phase {
            Phase::Available(r) | Phase::Installing(r) | Phase::Homebrew(r) => Some(r),
            Phase::Failed { release, .. } => Some(release),
            _ => None,
        }
    }
}

pub fn updates(cx: &App) -> Option<&Updates> {
    cx.try_global::<Updates>()
}

fn set_phase(phase: Phase, cx: &mut App) {
    if let Some(u) = cx.try_global::<Updates>() {
        let mut u = u.clone();
        u.phase = phase;
        cx.set_global(u);
        cx.refresh_windows();
    }
}

/// Learns whether this copy can update itself and, when it can, checks once a day while the
/// switch is on. The first check waits a minute, so starting the app makes no request at once.
pub fn start(cx: &mut App) {
    cx.set_global(Updates::detect());
    if updates(cx).is_none_or(|u| u.bundle.is_none()) {
        return;
    }
    cx.spawn(async move |cx| {
        let mut wait = Duration::from_secs(60);
        loop {
            cx.background_executor().timer(wait).await;
            wait = Duration::from_secs(3600);
            cx.update(check_if_due);
        }
    })
    .detach();
}

fn check_if_due(cx: &mut App) {
    let Some(u) = updates(cx) else { return };
    if !u.auto
        || !matches!(
            u.phase,
            Phase::Idle | Phase::Latest(_) | Phase::CheckFailed(_)
        )
    {
        return;
    }
    let Some(cache) = core::cache_dir() else {
        return;
    };
    if core::due(core::last_check(&cache), fibo_core::time::now()) {
        check(false, cx);
    }
}

/// Asks GitHub for the latest release. `manual` is a click on 检查更新: only then is a failure
/// shown; the daily check fails quietly and tries again tomorrow.
pub fn check(manual: bool, cx: &mut App) {
    let Some(u) = updates(cx) else { return };
    if u.bundle.is_none() || matches!(u.phase, Phase::Checking | Phase::Installing(_)) {
        return;
    }
    let current = u.current.clone();
    let before = u.phase.clone();
    set_phase(Phase::Checking, cx);
    cx.spawn(async move |cx| {
        let found = cx
            .background_spawn(async move {
                let url = core::latest_url(&core::api_base().0);
                let found = core::check(&url, &current, core::arch(), &system());
                if let Some(cache) = core::cache_dir() {
                    core::record_check(&cache, fibo_core::time::now());
                }
                found
            })
            .await;
        cx.update(|cx| {
            let bundle = updates(cx).and_then(|u| u.bundle.clone());
            let phase = match found {
                Ok(Found::Latest(v)) => Phase::Latest(v),
                Ok(Found::Newer(r))
                    if bundle
                        .is_some_and(|b| core::homebrew_managed(&b, &core::brew_prefixes())) =>
                {
                    Phase::Homebrew(r)
                }
                Ok(Found::Newer(r)) => Phase::Available(r),
                Err(e) if manual => Phase::CheckFailed(e),
                Err(_) => match before {
                    Phase::Checking => Phase::Idle,
                    other => other,
                },
            };
            // Test only, beside a fake release server: install what the check found, for a
            // run with no one at the screen to click.
            let unattended =
                !core::api_base().1 && std::env::var_os(core::AUTO_INSTALL_ENV).is_some();
            let install_now = unattended && matches!(phase, Phase::Available(_));
            set_phase(phase, cx);
            if install_now {
                install(cx);
            }
        });
    })
    .detach();
}

/// Turns the daily check on or off.
pub fn set_auto(on: bool, cx: &mut App) -> Result<(), String> {
    let file = core::auto_check_file().ok_or(t!("找不到配置目录"))?;
    file.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            crate::ui::status_colors::write_atomic(&file, if on { b"on" } else { b"off" })
        })
        .map_err(|e| t!("无法保存 {display}：{e}", display = file.display(), e))?;
    if let Some(u) = updates(cx) {
        let mut u = u.clone();
        u.auto = on;
        cx.set_global(u);
        cx.refresh_windows();
    }
    Ok(())
}

/// 立即更新: installs the offered release and restarts into it. Where this copy cannot be
/// replaced, or Homebrew owns it, the release page or the brew command is the answer instead.
pub fn install(cx: &mut App) {
    let Some(u) = updates(cx) else { return };
    let (Some(bundle), Some(release)) = (u.bundle.clone(), u.offered().cloned()) else {
        return;
    };
    match &u.phase {
        Phase::Available(_) => {}
        Phase::Failed { page: true, .. } => return cx.open_url(&release.page),
        Phase::Failed { .. } => {}
        _ => return,
    }
    set_phase(Phase::Installing(release.clone()), cx);
    cx.spawn(async move |cx| {
        let job_release = release.clone();
        let job_bundle = bundle.clone();
        let result = cx
            .background_spawn(async move {
                let cache = core::cache_dir()
                    .ok_or(core::Failure::Download(t!("找不到缓存目录").into()))?;
                let real = core::api_base().1;
                let job = core::Install {
                    release: &job_release,
                    bundle: &job_bundle,
                    cache: &cache,
                    brew_prefixes: &core::brew_prefixes(),
                    allow_unnotarized: !real && std::env::var_os(core::UNNOTARIZED_ENV).is_some(),
                };
                core::install(&job, &system(), &|step| {
                    eprintln!("update {}: {step:?}", job_release.version)
                })
            })
            .await;
        cx.update(|cx| match result {
            Ok(()) => relaunch(&bundle, cx),
            Err(core::Failure::Homebrew) => set_phase(Phase::Homebrew(release), cx),
            Err(failure) => {
                eprintln!("update {}: {}", release.version, failure.message());
                set_phase(
                    Phase::Failed {
                        page: failure.page_helps(),
                        message: failure.message(),
                        release,
                    },
                    cx,
                )
            }
        });
    })
    .detach();
}

/// Starts the new bundle once this process has gone, with the same arguments (`--runtime`),
/// and quits. The new app's host takes the terminals over from the one running now.
fn relaunch(bundle: &std::path::Path, cx: &mut App) {
    let program = bundle.join("Contents/MacOS/fibo-app");
    match fibo_platform::process::start_after_exit(
        std::process::id(),
        &program,
        std::env::args_os().skip(1),
    ) {
        Ok(()) => cx.quit(),
        // The new version is in place either way; the next start runs it.
        Err(e) => {
            let release = updates(cx).and_then(|u| u.offered().cloned());
            if let Some(release) = release {
                set_phase(
                    Phase::Failed {
                        release,
                        message: t!("已装好新版本，但没能自动重启（{e}），请退出后重新打开", e),
                        page: false,
                    },
                    cx,
                );
            }
        }
    }
}
