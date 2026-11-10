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
    ffi::{CStr, c_char},
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
/// The menu bar, notifications and Dock. macOS has them through AppKit; elsewhere they do not
/// exist yet, and the window does not depend on them.
#[cfg_attr(target_os = "macos", path = "desktop_macos.rs")]
#[cfg_attr(not(target_os = "macos"), path = "desktop_other.rs")]
mod native;
#[cfg(feature = "visual-test")]
pub use native::snapshot;
pub use native::{request_attention, route_applied, set_dock_icon, show};
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
/// A session's news for a banner: its dedupe key, the state, and the line under the title.
/// Waiting and failed sessions call you; a finished turn (Review) gets a banner too, but
/// no sound: it does not block anything.
fn fact(w: &Workspace, s: &AgentSession) -> Option<(String, State, String)> {
    let c = w.content.get(&s.id)?;
    // Banners follow the one count of 要你处理: nothing the window would not list.
    if !status::needs_you(w, s, now()) {
        return None;
    }
    let group = status::of(w, s, now());
    let body = match group {
        State::Waiting => c.waiting.clone(),
        State::Error => c.error.clone(),
        State::Review => c.headline(),
        _ => return None,
    };
    // One dialog is one event however many signals report it or refine its text; an error
    // is keyed by its text; a finished turn by the turn.
    let event = match group {
        State::Waiting => format!("episode {}", c.waiting_episode),
        State::Review => String::new(),
        _ => body.clone(),
    };
    Some((
        format!("{:?}:{}:{}:{}", group, c.turn_key, c.turn_started, event),
        group,
        body,
    ))
}
/// The banner's title, as the landing page shows it: who and what, e.g.
/// "Claude Code is ready for review".
fn banner_title(agent: &str, group: State) -> String {
    match group {
        State::Waiting => t!("{agent} 有话问你", agent),
        State::Error => t!("{agent} 中途报错", agent),
        _ => t!("{agent} 这轮好了，请你过目", agent),
    }
}
/// A banner to post: session, dedupe key, title, body, and whether it may sound.
pub(crate) struct Banner {
    pub session: String,
    pub key: String,
    pub title: String,
    pub body: String,
    pub urgent: bool,
}
fn entered(before: &Workspace, after: &Workspace, watching: Option<&str>) -> Vec<Banner> {
    let (notify, sound) = (after.settings.notifications, after.settings.sound);
    if !notify && !sound {
        return vec![];
    }
    after
        .agent_sessions
        .iter()
        .filter(|s| s.managed() && (watching.is_none() || s.terminal.as_deref() != watching))
        // Taking a space back is not news: what waited there comes back as counts, without
        // a burst of banners; its next new fact is a banner again.
        .filter(|s| !before.session_quiet(s))
        .filter_map(|s| {
            let (key, group, body) = fact(after, s)?;
            // The sound alone only speaks for what blocks the Agent.
            if !notify && !group.blocked() {
                return None;
            }
            let old = before
                .agent_sessions
                .iter()
                .find(|p| p.id == s.id)
                .and_then(|s| fact(before, s));
            if old.as_ref().is_some_and(|(old, ..)| old == &key) {
                return None;
            }
            // Name the project first: several panes often run the same kind of task.
            let detail = match short(&body, 180) {
                d if d.is_empty() => short(&after.session_title(s), 72),
                d => d,
            };
            let body = match space_name(after, s) {
                Some(space) => format!("{space} · {detail}"),
                None => detail,
            };
            Some(Banner {
                session: s.id.clone(),
                key,
                title: banner_title(s.runtime.name(), group),
                body,
                urgent: group.blocked(),
            })
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
        if c.hidden_for(s).is_some() || w.session_quiet(s) {
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
        (
            t!("连不上后台").to_owned(),
            t!("数据停在 {stopped}", stopped),
        )
    } else if total == 0 {
        (
            t!("没有{needs}的", needs = status::needs_you_word()),
            t!("刚刚更新").to_owned(),
        )
    } else {
        (
            t!("{total} 项{needs}", total, needs = status::needs_you_word()),
            t!("刚刚更新").to_owned(),
        )
    };
    let (section, section_note) = if offline {
        (
            t!(
                "{stopped} 时{needs}",
                stopped,
                needs = status::needs_you_word()
            ),
            t!("可能已经变了"),
        )
    } else {
        (status::needs_you_word().to_owned(), t!("等你在前，按时间"))
    };
    // The empty view says what happens next instead of leaving a blank.
    let empty = (!offline && total == 0).then(|| {
        // Whole sentences: who stops and what happens then, in each language's order.
        let title = match (running > 0, w.settings.notifications, w.settings.sound) {
            (true, true, _) => t!(
                "{state}的 {running} 个停下来要你时，会发系统通知",
                running,
                state = State::Running.label()
            ),
            (true, false, true) => t!(
                "{state}的 {running} 个停下来要你时，会响提示音",
                running,
                state = State::Running.label()
            ),
            (true, false, false) => t!(
                "{state}的 {running} 个停下来要你时，会出现在这里",
                running,
                state = State::Running.label()
            ),
            (false, true, _) => t!("有 Agent 停下来要你时，会发系统通知").to_owned(),
            (false, false, true) => t!("有 Agent 停下来要你时，会响提示音").to_owned(),
            (false, false, false) => t!("有 Agent 停下来要你时，会出现在这里").to_owned(),
        };
        let note = match (w.settings.notifications, w.settings.sound) {
            (true, _) => t!("通知已开 · 你正看着的画面不会再弹"),
            (false, true) => t!("系统通知没开 · 在设置的通知页打开"),
            (false, false) => t!("通知没开 · 在设置的通知页打开"),
        };
        json!({"title": title, "note": note})
    });
    let (foot, foot_count, foot_note) = if offline {
        (t!("重连后自动刷新"), None, t!("通知暂停"))
    } else {
        (State::Stale.label(), Some(stale), t!("外部会话在鸟瞰里看"))
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
            "waiting": State::Waiting.short_label(), "errors": State::Error.short_label(),
            "review": State::Review.short_label(), "running": State::Running.short_label(),
        },
        // The panel's own words, in the window's language.
        "words": {
            "open": t!("打开工作台"), "copy_error": t!("复制错误"),
            "diagnostics": t!("查看诊断"), "reconnecting": t!("正在重连"),
            "offline": t!("连不上后台，正在重连："), "more": t!("还有 %ld 项"),
            "offline_tip": t!("Fibo · 连不上后台"),
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
pub fn start(socket: &Path, initial: Workspace, cx: &mut App) -> UnboundedReceiver<String> {
    let (tx, events) = unbounded();
    let _ = EVENTS.set(tx);
    cx.set_global(Status::default());
    // All AppKit entry points run on the GPUI/main thread.
    native::install(event);
    native::update(&panel(&initial, &Link::fresh(RecognitionPhase::Starting)));
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
            let status: RecognitionStatus = crate::connection::StatusLink::into_status(status);
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
                    cx.update(|_| native::update(&value));
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
                cx.update(|_| native::update(&value));
                previous_panel = value;
            }
            if !before.settings.notifications && after.settings.notifications {
                cx.update(|_| native::notifications_enable());
            }
            if ready {
                for session in left(&before, &after) {
                    cx.update(|_| native::withdraw(&session));
                }
                let watching =
                    cx.update(|cx| cx.try_global::<Watching>().and_then(|w| w.0.clone()));
                for banner in entered(&before, &after, watching.as_deref()) {
                    let Banner {
                        session,
                        key,
                        title,
                        body,
                        urgent,
                    } = banner;
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
                            cx.update(|_| {
                                if settings.notifications {
                                    native::notify(
                                        &session,
                                        &title,
                                        &body,
                                        settings.sound && urgent,
                                    );
                                } else {
                                    native::play_sound();
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
