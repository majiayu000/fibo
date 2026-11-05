//! Event-driven macOS status item and notification delivery; no timers or process probes.
use crate::{connection::RecognitionConnection, ui::theme::Theme as T};
use futures::{
    StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
};
use gpui::{App, AppContext, Global};
use mooring_core::{
    content::{now, short},
    model::*,
    protocol::*,
    status::{self, Status as State},
};
use serde_json::json;
use std::{
    ffi::{CStr, CString, c_char},
    path::Path,
    sync::OnceLock,
};

#[derive(Default)]
pub struct Status(pub String);
impl Global for Status {}
const MENUBAR_FAILED: &str = "菜单栏连接失败：";
/// The route the panel's 查看诊断 sends; session ids never take this form.
pub const DIAGNOSTICS: &str = "diagnostics:";
/// The terminal in the focused pane of the active window, if any.
#[derive(Default)]
pub struct Watching(pub Option<String>);
impl Global for Watching {}
static EVENTS: OnceLock<UnboundedSender<String>> = OnceLock::new();
unsafe extern "C" {
    fn kl_desktop_route_applied(
        session: *const c_char,
        terminal: *const c_char,
        tab: *const c_char,
    );
    fn kl_desktop_install(callback: extern "C" fn(*const c_char));
    fn kl_desktop_update(json: *const c_char);
    fn kl_notifications_enable();
    fn kl_desktop_show();
    fn kl_request_attention();
    fn kl_set_dock_icon(png: *const u8, length: usize);
    fn kl_desktop_notify(
        session: *const c_char,
        title: *const c_char,
        body: *const c_char,
        sound: i32,
    );
    fn kl_play_sound();
    fn kl_desktop_withdraw(session: *const c_char);
}
pub fn set_dock_icon(png: &[u8]) {
    unsafe { kl_set_dock_icon(png.as_ptr(), png.len()) }
}
pub fn route_applied(session: &str, terminal: &str, tab: &str) {
    unsafe {
        kl_desktop_route_applied(
            cstring(session).as_ptr(),
            cstring(terminal).as_ptr(),
            cstring(tab).as_ptr(),
        );
    }
}
extern "C" fn event(value: *const c_char) {
    if value.is_null() {
        return;
    }
    // Native strings live through the callback; copy before returning to AppKit.
    let value = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    if let Some(tx) = EVENTS.get() {
        let _ = tx.unbounded_send(value);
    }
}
fn cstring(value: &str) -> CString {
    CString::new(value.replace('\0', " ")).expect("removed NUL")
}
fn fact(w: &Workspace, s: &AgentSession) -> Option<(String, String)> {
    let c = w.content.get(&s.id)?;
    // Banners follow the one count of 要你处理: nothing the window would not list.
    if !status::needs_you(w, s, now()) {
        return None;
    }
    let group = status::of(w, s, now());
    let body = match group {
        State::Waiting => c.waiting.clone(),
        State::Error => c.error.clone(),
        _ => return None,
    };
    // One dialog is one event however many signals report it or refine its text; an error
    // is keyed by its text.
    let event = match group {
        State::Waiting => format!("episode {}", c.waiting_episode),
        _ => body.clone(),
    };
    Some((
        format!("{:?}:{}:{}:{}", group, c.turn_key, c.turn_started, event),
        format!("{} · {}", group.label(), short(&body, 180)),
    ))
}
fn entered(
    before: &Workspace,
    after: &Workspace,
    watching: Option<&str>,
) -> Vec<(String, String, String, String)> {
    if !after.settings.notifications && !after.settings.sound {
        return vec![];
    }
    after
        .agent_sessions
        .iter()
        .filter(|s| s.managed() && (watching.is_none() || s.terminal.as_deref() != watching))
        .filter_map(|s| {
            let (key, body) = fact(after, s)?;
            let old = before
                .agent_sessions
                .iter()
                .find(|p| p.id == s.id)
                .and_then(|s| fact(before, s));
            if old.as_ref().is_some_and(|(old, _)| old == &key) {
                return None;
            }
            let title = short(&after.session_title(s), 72);
            // Name the project first: several panes often run the same kind of task.
            let title = match space_name(after, s) {
                Some(space) => format!("{space} · {title}"),
                None => title,
            };
            Some((s.id.clone(), key, title, body))
        })
        .collect()
}
/// Sessions that needed you and no longer do: answered here or in the terminal, or gone.
/// Their banner would otherwise sit in Notification Center asking for what is done.
fn left(before: &Workspace, after: &Workspace) -> Vec<String> {
    before
        .agent_sessions
        .iter()
        .filter(|s| fact(before, s).is_some())
        .filter(|s| {
            after
                .agent_sessions
                .iter()
                .find(|a| a.id == s.id)
                .is_none_or(|a| fact(after, a).is_none())
        })
        .map(|s| s.id.clone())
        .collect()
}
/// The project a session's terminal belongs to, as notifications and the panel name it.
fn space_name(w: &Workspace, s: &AgentSession) -> Option<String> {
    w.terminals
        .iter()
        .find(|t| Some(&t.id) == s.terminal.as_ref())
        .and_then(|t| w.spaces.iter().find(|p| p.id == t.space))
        .map(|p| p.name.clone())
}
/// What the menu bar knows about its own link to the host: the panel says when its
/// numbers stopped being live instead of showing old ones as current.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Link {
    phase: RecognitionPhase,
    /// The last refresh's failure, while the host cannot be reached.
    error: Option<String>,
    /// When the current state began: connected since, or, while the host cannot be
    /// reached, the last time the panel's workspace was fresh.
    since: u64,
}
impl Link {
    fn fresh(phase: RecognitionPhase) -> Self {
        Self {
            phase,
            error: None,
            since: now(),
        }
    }
    fn phase(&self) -> &'static str {
        match (&self.error, self.phase) {
            (None, RecognitionPhase::Starting) => "starting",
            (None, _) => "healthy",
            (Some(_), RecognitionPhase::Recovering) => "recovering",
            (Some(_), _) => "interrupted",
        }
    }
}
/// `13:04`, the minute the panel's data dates from.
fn minute(at: u64) -> String {
    let mut clock = crate::model::time::clock(at);
    clock.truncate(5);
    clock
}
/// Rows the panel lists; the rest are counted under them.
const SHOWN: usize = 5;
fn panel(w: &Workspace, link: &Link) -> String {
    let mut rows = Vec::new();
    let mut review = 0;
    let mut running = 0;
    let mut stale = 0;
    let mut waiting = 0;
    let mut errors = 0;
    let at = now();
    for s in &w.agent_sessions {
        let Some(c) = w.content.get(&s.id) else {
            continue;
        };
        if c.hidden_for(s).is_some() {
            continue;
        }
        let group = status::of(w, s, at);
        let attention = status::needs_you(w, s, at);
        // Waiting or failed but not yours to answer (outside Mooring, put off, closed
        // tab): counted nowhere, as the window does.
        if group.needs_you() && !attention {
            continue;
        }
        match group {
            State::Review => review += 1,
            g if g.working() => running += 1,
            State::Stale => stale += 1,
            State::Waiting => waiting += 1,
            State::Error => errors += 1,
            _ => (),
        };
        if attention {
            let meta = match space_name(w, s) {
                Some(space) => format!("{space} · {}", s.runtime.name()),
                None => s.runtime.name().to_owned(),
            };
            let waits = group == State::Waiting;
            let kind = match group {
                State::Waiting => "wait",
                State::Error => "error",
                _ => "review",
            };
            let summary = match group {
                State::Waiting => c.waiting.clone(),
                State::Error => c.error.clone(),
                _ => c.headline(),
            };
            rows.push((
                // Asking first, then failed, then finished; newest first within each.
                (
                    !waits,
                    group != State::Error,
                    std::cmp::Reverse(w.session_updated(s)),
                ),
                json!({
                    "id": s.id,
                    "title": short(&w.session_title(s), 24),
                    "summary": short(&summary, 40),
                    "age": crate::model::time::age(w.session_updated(s)),
                    "kind": kind,
                    "meta": meta,
                }),
            ));
        }
    }
    // Waiting first, then the newest.
    rows.sort_by_key(|r| r.0);
    let total = rows.len();
    let offline = link.error.is_some();
    let stopped = minute(link.since);
    let (heading, when) = if offline {
        ("连不上后台".to_owned(), format!("数据停在 {stopped}"))
    } else if total == 0 {
        (
            format!("没有{}的", status::NEEDS_YOU),
            "刚刚更新".to_owned(),
        )
    } else {
        (
            format!("{total} 项{}", status::NEEDS_YOU),
            "刚刚更新".to_owned(),
        )
    };
    let (section, section_note) = if offline {
        (format!("{stopped} 时{}", status::NEEDS_YOU), "可能已经变了")
    } else {
        (status::NEEDS_YOU.to_owned(), "等你在前，按时间")
    };
    // The empty view says what happens next instead of leaving a blank.
    let empty = (!offline && total == 0).then(|| {
        let who = if running > 0 {
            format!("在跑的 {running} 个停下来要你时")
        } else {
            "有 Agent 停下来要你时".to_owned()
        };
        let (what, note) = match (w.settings.notifications, w.settings.sound) {
            (true, _) => ("会发系统通知", "通知已开 · 你正看着的画面不会再弹"),
            (false, true) => ("会响提示音", "系统通知没开 · 在设置的通知页打开"),
            (false, false) => ("会出现在这里", "通知没开 · 在设置的通知页打开"),
        };
        json!({"title": format!("{who}，{what}"), "note": note})
    });
    let (foot, foot_count, foot_note) = if offline {
        ("重连后自动刷新", None, "通知暂停")
    } else {
        ("很久没动", Some(stale), "外部会话在鸟瞰里看")
    };
    json!({
        "badge": if offline || total == 0 { String::new() } else { total.to_string() },
        "heading": heading,
        "when": when,
        "needs": total,
        "counts": {
            "waiting": waiting, "errors": errors, "review": review,
            "running": running, "stale": stale,
        },
        // The words come from the one table every surface uses (`Status::label`).
        "labels": {
            "waiting": State::Waiting.label(), "errors": State::Error.label(),
            "review": State::Review.label(), "running": State::Running.label(),
        },
        "section": section,
        "section_note": section_note,
        "rows": rows.into_iter().take(SHOWN).map(|r| r.1).collect::<Vec<_>>(),
        "more": total.saturating_sub(SHOWN),
        "empty": empty,
        "foot": foot,
        "foot_count": foot_count,
        "foot_note": foot_note,
        "notifications": w.settings.notifications,
        // The subscription retries on its own and does not count its attempts.
        "connection": {"phase": link.phase(), "error": link.error, "since": link.since},
        "tokens": {
            "width": T::MENUBAR_WIDTH, "height": T::MENUBAR_HEIGHT, "gap": T::S12,
            "font": T::BODY_SIZE, "title": T::MENU_TITLE, "count": T::MENU_COUNT,
            "small": T::SMALL, "meta": T::META, "chip": T::CHIP_SIZE,
            "button": T::MENU_BUTTON, "radius": T::S8,
            "light": T::menu_light(), "dark": T::menu_dark(),
        },
    })
    .to_string()
}
pub fn show() {
    unsafe {
        kl_desktop_show();
    }
}
pub fn request_attention() {
    unsafe {
        kl_request_attention();
    }
}
pub fn start(socket: &Path, initial: Workspace, cx: &mut App) -> UnboundedReceiver<String> {
    let (tx, events) = unbounded();
    let _ = EVENTS.set(tx);
    cx.set_global(Status::default());
    // All AppKit entry points run on the GPUI/main thread.
    unsafe {
        kl_desktop_install(event);
        kl_desktop_update(
            cstring(&panel(&initial, &Link::fresh(RecognitionPhase::Starting))).as_ptr(),
        );
    }
    let (status_tx, mut status_rx) = unbounded();
    let connection = RecognitionConnection::new(socket, status_tx);
    let socket = socket.to_owned();
    cx.spawn(async move |cx| {
        let _connection = connection;
        let mut before = initial;
        let mut ready = false;
        let mut link = Link::fresh(RecognitionPhase::Starting);
        let mut previous_panel = panel(&before, &link);
        let mut fresh = now();
        while let Some(status) = status_rx.next().await {
            let socket_read = socket.clone();
            let result = cx
                .background_spawn(async move { rpc(&socket_read, &Request::Workspace) })
                .await;
            link.phase = status.phase;
            if let Err(e) = &result {
                // Keep the last numbers on screen, marked with the minute they date from.
                if link.error.is_none() {
                    link.since = fresh;
                }
                link.error = Some(e.to_string());
                let value = panel(&before, &link);
                if previous_panel != value {
                    cx.update(|_| unsafe {
                        kl_desktop_update(cstring(&value).as_ptr());
                    });
                    previous_panel = value;
                }
            }
            let after = match result {
                Ok(Reply::Workspace(w)) => {
                    // A host restart breaks one refresh; clear its note once the next one works.
                    cx.update(|cx| {
                        if cx.global::<Status>().0.starts_with(MENUBAR_FAILED) {
                            cx.set_global(Status::default());
                        }
                    });
                    w
                }
                Err(e) => {
                    cx.update(|cx| cx.set_global(Status(format!("{MENUBAR_FAILED}{e}"))));
                    continue;
                }
                _ => continue,
            };
            fresh = now();
            if link.error.take().is_some() {
                link.since = fresh;
            }
            let value = panel(&after, &link);
            if previous_panel != value {
                cx.update(|_| unsafe {
                    kl_desktop_update(cstring(&value).as_ptr());
                });
                previous_panel = value;
            }
            if !before.settings.notifications && after.settings.notifications {
                cx.update(|_| unsafe { kl_notifications_enable() });
            }
            if ready {
                for session in left(&before, &after) {
                    cx.update(|_| unsafe { kl_desktop_withdraw(cstring(&session).as_ptr()) });
                }
                let watching =
                    cx.update(|cx| cx.try_global::<Watching>().and_then(|w| w.0.clone()));
                for (session, key, title, body) in entered(&before, &after, watching.as_deref()) {
                    let socket_claim = socket.clone();
                    let claim_session = session.clone();
                    match cx
                        .background_spawn(async move {
                            rpc(
                                &socket_claim,
                                &Request::ClaimNotification {
                                    session: claim_session,
                                    key,
                                },
                            )
                        })
                        .await
                    {
                        Ok(Reply::NotificationClaimed(true)) => {
                            let settings = &after.settings;
                            cx.update(|_| unsafe {
                                if settings.notifications {
                                    kl_desktop_notify(
                                        cstring(&session).as_ptr(),
                                        cstring(&title).as_ptr(),
                                        cstring(&body).as_ptr(),
                                        i32::from(settings.sound),
                                    );
                                } else {
                                    kl_play_sound();
                                }
                            });
                        }
                        Err(e) => {
                            cx.update(|cx| cx.set_global(Status(format!("通知去重记录失败：{e}"))));
                        }
                        _ => (),
                    }
                }
            }
            ready |= status.phase == RecognitionPhase::Healthy;
            before = after;
        }
    })
    .detach();
    events
}
#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;

#[cfg(feature = "visual-test")]
pub fn snapshot(path: &Path) {
    unsafe extern "C" {
        fn kl_desktop_snapshot(path: *const c_char) -> bool;
    }
    assert!(
        unsafe { kl_desktop_snapshot(cstring(&path.to_string_lossy()).as_ptr()) },
        "native menu panel export"
    );
}
