#[path = "pane_ask.rs"]
mod ask;
#[path = "pane_handoff.rs"]
mod handoff;
#[path = "pane_ime.rs"]
mod ime;
#[path = "pane_input.rs"]
mod input;
#[path = "pane_links.rs"]
mod links;
#[path = "pane_mouse.rs"]
mod mouse;
#[path = "pane_overlays.rs"]
mod overlays;
#[path = "pane_paste.rs"]
mod paste;
#[path = "pane_render.rs"]
mod render;
#[path = "pane_search.rs"]
mod search;
#[path = "pane_state.rs"]
mod state;
#[cfg(feature = "visual-test")]
#[path = "pane_visual.rs"]
pub mod visual;
use crate::{
    connection::Connection,
    terminal::screen::{self, Screen},
};
use futures::{StreamExt, channel::mpsc::unbounded};
use gpui::{App, Bounds, Context, FocusHandle, Focusable, Pixels, Window, prelude::*};
use mooring_core::protocol::{Reply, Request, rpc};
use state::{AskPin, Handoff, Links, Search, Selection};
use std::{ops::Range, path::Path, sync::atomic::Ordering, time::Instant};

/// The strip along a pane's right edge that grabs the scrollbar, and the thumb's width.
const SCROLLBAR_HIT: f32 = 10.;
const SCROLLBAR_WIDTH: f32 = 6.;
/// Wheel reports sent to a program for one scroll event.
const WHEEL_REPORTS: usize = 10;
/// How often output may re-ask the host for the program's directory.
const CWD_REFRESH_SECS: u64 = 5;
/// The floating ask stays up this long after the last scroll through history.
const ASK_FLOAT_MS: u64 = 1500;
/// Output re-reads where the asks are at most this often; any other repaint, every
/// few seconds (history may have dropped its oldest rows, moving the rest up).
const ASK_LOCATE_MS: u64 = 500;
const ASK_RELOCATE_MS: u64 = 3000;
pub struct Pane {
    terminal: String,
    socket: std::path::PathBuf,
    /// Set for another machine's terminal: where its live connection is found, read
    /// only. Reattaching takes whatever connection that machine has then.
    remote: Option<crate::machines::Slot>,
    /// Wakes this pane on terminal output; every connection it opens shares it.
    wake: futures::channel::mpsc::UnboundedSender<()>,
    connection: Connection,
    /// A dropped stream is being reattached; the delay doubles while it keeps failing.
    reconnecting: bool,
    reconnect_delay: u64,
    focus: FocusHandle,
    screen: Screen,
    capture: Option<screen::Capture>,
    layout_cache: screen::LayoutCache,
    metrics_font: Option<(gpui::Font, f32, f32, f32)>,
    marked: String,
    selection: Range<usize>,
    bounds: Option<Bounds<Pixels>>,
    metrics: Option<screen::CellMetrics>,
    cursor_anchor: screen::CursorAnchor,
    requested_size: (u16, u16),
    /// Device-pixel cell size last sent with the size, for the program's size reports.
    requested_cell: (u32, u32),
    local_error: Option<String>,
    handoff: Option<Handoff>,
    select: Selection,
    /// Backing scale of the last paint, for drags reported outside the pane.
    scale: f32,
    mouse: Option<crate::terminal::terminal_input::MouseEncoder>,
    scroll_lines: f32,
    scroll_cols: f32,
    /// The viewport is in history; a chip offers the way back to the live screen.
    scrolled_back: bool,
    /// The viewport's first row, the scrollable rows and the shown rows, read each frame;
    /// rows count from the top of history, as search matches do.
    viewport: (usize, usize, usize),
    links: Links,
    /// The program's working directory, asked from the host when a relative path is hovered.
    cwd: Option<std::path::PathBuf>,
    /// When the footer last asked the host for that directory; output re-asks at most
    /// every few seconds, and an idle pane asks nothing.
    cwd_asked: Option<Instant>,
    /// An ask held back by the throttle is still to come.
    cwd_due: bool,
    focus_observers: bool,
    last_focus: bool,
    /// The focused cursor's blink phase; a keystroke shows it and restarts the count.
    blink_on: bool,
    /// Bumped to retire the running blink timer.
    blink_epoch: u64,
    /// When the user last sent input; its echo is drawn without waiting for the shared tick.
    typed: Option<Instant>,
    search: Search,
    /// A multi-line paste waiting for the user's go-ahead (no bracketed paste).
    paste_confirm: Option<String>,
    bells_seen: u64,
    /// The program rang while the user looked elsewhere; the header shows it until focus.
    pub bell: bool,
    /// A bell in the pane the user is looking at flashes its border once.
    flash: bool,
    /// A short note in the corner that clears itself, e.g. how to select when the
    /// program took the mouse.
    toast: Option<gpui::SharedString>,
    toast_seq: u64,
    /// The Agent running in this terminal, as the workbench last saw it.
    pub agent: Option<mooring_core::model::Agent>,
    /// Shares the tab with other panes: unless it holds focus, it fades and draws no
    /// cursor, so only one cursor is in sight.
    pub split: bool,
    /// Shown in the peek overlay and only looked at: it does not resize the terminal
    /// (that would reflow the Agent's screen for its own tab) and keeps the bottom rows
    /// in view when taller. The first key, paste or click in it clears this, so the
    /// overlay takes the size: the client used last decides the size.
    pub peek: bool,
    /// The scheme this pane first painted its live program with. Agents such as Codex
    /// tint their own bands from the background they saw at start, which a later switch
    /// cannot repaint, so the pane says so until dismissed.
    started_under: Option<mooring_core::terminal::TermColors>,
    tint_note_dismissed: bool,
    ask: AskPin,
}
/// A hovered path word that exists on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct Hovered {
    pub path: std::path::PathBuf,
    /// The line and column the word named, as in `pane.rs:651:7`.
    pub at: Option<(u32, Option<u32>)>,
    /// The whole word named it, not one piece of a word broken over rows, so every
    /// piece is underlined.
    pub whole: bool,
    /// The program's directory when hovered, for copying the path relative to it.
    pub cwd: Option<std::path::PathBuf>,
}
/// What the pane asks of the workbench around it.
pub enum PaneEvent {
    /// Run `command` in a new terminal tab started in `cwd`.
    Run { command: String, cwd: String },
    /// The turn's ask scrolled out of sight or back, for the header's 「↑ 你问」.
    PinnedAsk,
}
impl gpui::EventEmitter<PaneEvent> for Pane {}
impl Focusable for Pane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Pane {
    pub fn new(socket: &Path, id: &str, takeover: bool, cx: &mut Context<Self>) -> Self {
        let (tx, rx) = unbounded();
        let connection = Connection::new(socket, id, takeover, tx.clone());
        Self::with(connection, socket.to_owned(), None, id, tx, rx, cx)
    }
    /// Another machine's terminal, looked at until it is used (`take_size`).
    pub fn remote(slot: crate::machines::Slot, id: &str, cx: &mut Context<Self>) -> Self {
        let (tx, rx) = unbounded();
        let linked = slot.lock().unwrap().clone();
        let connection = Connection::remote(linked, id, false, tx.clone());
        let mut pane = Self::with(connection, Default::default(), Some(slot), id, tx, rx, cx);
        pane.peek = true;
        pane
    }
    fn with(
        connection: Connection,
        socket: std::path::PathBuf,
        remote: Option<crate::machines::Slot>,
        id: &str,
        tx: futures::channel::mpsc::UnboundedSender<()>,
        mut rx: futures::channel::mpsc::UnboundedReceiver<()>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                // Coalesce bursts at 60 Hz; no timer exists when the terminal is idle.
                // Output nobody is typing into waits for the shared tick instead, so
                // several busy panes repaint in one frame rather than one frame each.
                let echo = this
                    .read_with(cx, |pane, _| {
                        pane.typed
                            .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(300))
                    })
                    .unwrap_or(false);
                cx.background_executor()
                    .timer(if echo {
                        std::time::Duration::from_millis(16)
                    } else {
                        until_shared_tick()
                    })
                    .await;
                while rx.try_recv().is_ok() {}
                if this
                    .update(cx, |pane, cx| {
                        pane.ring(cx);
                        pane.recover(cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let focus = cx.focus_handle();
        Self {
            terminal: id.into(),
            socket,
            remote,
            wake: tx,
            connection,
            reconnecting: false,
            reconnect_delay: 0,
            focus,
            screen: Screen::default(),
            capture: None,
            layout_cache: screen::LayoutCache::default(),
            metrics_font: None,
            marked: String::new(),
            selection: 0..0,
            bounds: None,
            metrics: None,
            cursor_anchor: screen::CursorAnchor::default(),
            requested_size: (0, 0),
            requested_cell: (0, 0),
            local_error: None,
            handoff: None,
            select: Selection::new(),
            scale: 1.,
            mouse: None,
            scroll_lines: 0.,
            scroll_cols: 0.,
            scrolled_back: false,
            viewport: (0, 0, 0),
            links: Links::default(),
            cwd: None,
            cwd_asked: None,
            cwd_due: false,
            focus_observers: false,
            last_focus: false,
            blink_on: true,
            blink_epoch: 0,
            typed: None,
            search: Search::default(),
            paste_confirm: None,
            bells_seen: 0,
            bell: false,
            flash: false,
            toast: None,
            toast_seq: 0,
            agent: None,
            split: false,
            peek: false,
            started_under: None,
            tint_note_dismissed: false,
            ask: AskPin::default(),
        }
    }
    /// Attaches again after the host dropped this pane's stream; the snapshot restores
    /// the screen, and input works again once the pane owns the terminal.
    fn recover(&mut self, cx: &mut Context<Self>) {
        let (dropped, failed, live) =
            self.connection
                .state
                .lock()
                .map_or((false, false, false), |s| {
                    (
                        s.dropped,
                        s.error.is_some() && !s.ended,
                        s.vt.is_some() && s.error.is_none(),
                    )
                });
        if live {
            self.reconnect_delay = 0;
        }
        // Once dropped, every failed attempt retries until one restores the screen: an
        // attempt made during the same burst is dropped again before its snapshot lands.
        let dropped = dropped || (self.reconnect_delay > 0 && failed);
        if !dropped || self.reconnecting {
            return;
        }
        self.reconnecting = true;
        let delay = self.reconnect_delay.max(100);
        self.reconnect_delay = (delay * 2).min(5000);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(delay))
                .await;
            let _ = this.update(cx, |pane, cx| {
                pane.connection = match &pane.remote {
                    Some(slot) => {
                        let linked = slot.lock().unwrap().clone();
                        Connection::remote(linked, &pane.terminal, !pane.peek, pane.wake.clone())
                    }
                    None => Connection::new(&pane.socket, &pane.terminal, false, pane.wake.clone()),
                };
                pane.reconnecting = false;
                pane.bells_seen = 0;
                pane.requested_cell = (0, 0);
                pane.local_error = None;
                cx.notify();
            });
        })
        .detach();
    }
    fn ring(&mut self, cx: &mut Context<Self>) {
        let rung = self.connection.bells.load(Ordering::Relaxed);
        if rung == self.bells_seen {
            return;
        }
        self.bells_seen = rung;
        if self.last_focus {
            self.flash = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
                let _ = this.update(cx, |pane, cx| {
                    pane.flash = false;
                    cx.notify();
                });
            })
            .detach();
        } else {
            self.bell = true;
            // Bounces the Dock icon once, only while Mooring is not the active app.
            crate::desktop::request_attention();
        }
    }
    /// The title the program set (OSC 0/2), e.g. a shell's "user@host: ~/dir" or vim's.
    pub fn program_title(&self) -> Option<String> {
        self.connection.state.lock().ok()?.vt.as_ref()?.title()
    }
    /// Dropped files arrive as their paths, escaped for a shell, like any terminal.
    fn drop_paths(&mut self, paths: &gpui::ExternalPaths, cx: &mut Context<Self>) {
        let text = crate::terminal::terminal_input::dropped_paths(paths.paths());
        if !text.is_empty() {
            self.prefill(&text, cx);
        }
    }
    fn sync_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = window.is_window_active() && self.focus.is_focused(window);
        if self.last_focus == focused {
            return;
        }
        self.last_focus = focused;
        self.restart_blink(cx);
        // Notifications skip the terminal the user is already looking at.
        let watching = cx
            .try_global::<crate::desktop::Watching>()
            .and_then(|w| w.0.clone());
        if focused {
            self.bell = false;
            cx.set_global(crate::desktop::Watching(Some(self.terminal.clone())));
        } else if watching.as_deref() == Some(self.terminal.as_str()) {
            cx.set_global(crate::desktop::Watching(None));
        }
        let bytes = {
            let state = self.connection.state.lock().unwrap();
            state
                .vt
                .as_ref()
                .filter(|_| state.writable)
                .map(|v| crate::terminal::terminal_input::focus_bytes(v, focused))
        };
        match bytes {
            // A focus report is not typing; it leaves the scrollback where it is.
            Some(Ok(b)) if !b.is_empty() => self.send_input(b, false, cx),
            Some(Err(e)) => {
                self.local_error = Some(e.to_string());
                cx.notify();
            }
            _ => (),
        }
        cx.notify(); // Invalidate the cached subtree when focus/activation changes.
    }
    /// The program's working directory as the host last reported it.
    pub fn cwd(&self) -> Option<&std::path::Path> {
        self.cwd.as_deref()
    }
    /// Asks the host where the program is now, for the footer; throttled, so a `cd` shows
    /// within a few seconds of the next output.
    fn refresh_cwd(&mut self, cx: &mut Context<Self>) {
        let window = std::time::Duration::from_secs(CWD_REFRESH_SECS);
        if let Some(at) = self.cwd_asked
            && at.elapsed() < window
        {
            // This output may be a `cd`'s last before the shell sits at its prompt: ask
            // once the window ends instead of waiting for output that may never come.
            if !self.cwd_due {
                self.cwd_due = true;
                let wait = window.saturating_sub(at.elapsed());
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    this.update(cx, |pane, cx| {
                        pane.cwd_due = false;
                        pane.refresh_cwd(cx);
                    })
                    .ok();
                })
                .detach();
            }
            return;
        }
        self.cwd_asked = Some(Instant::now());
        let socket = self.socket.clone();
        let id = self.terminal.clone();
        cx.spawn(async move |this, cx| {
            let reply = cx
                .background_spawn(async move { rpc(&socket, &Request::TerminalCwd { id }) })
                .await;
            if let Ok(Reply::Cwd(cwd)) = reply {
                this.update(cx, |pane, cx| {
                    let cwd = std::path::PathBuf::from(cwd);
                    if pane.cwd.as_ref() != Some(&cwd) {
                        pane.cwd = Some(cwd);
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }
    pub fn has_other_owner(&self) -> bool {
        self.connection
            .state
            .lock()
            .is_ok_and(|s| s.vt.is_some() && !s.writable && !s.ended && s.error.is_none())
    }
    fn show_toast(&mut self, text: impl Into<gpui::SharedString>, cx: &mut Context<Self>) {
        self.toast = Some(text.into());
        self.toast_seq += 1;
        let seq = self.toast_seq;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(3))
                .await;
            let _ = this.update(cx, |pane, cx| {
                // A newer note restarts the clock instead of being cut short.
                if pane.toast_seq == seq {
                    pane.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HandoffState {
    Waiting,
    Placed,
    /// Pasted, and its Enter is about to follow.
    Sending,
    NotReady,
    Missing,
}
const SELECT_HINT: &str = "这个程序接管了鼠标：按住 Shift 再拖动来选中文字";
/// Time left until the next boundary of one process-wide 50 ms clock: background panes
/// that wake at different moments all flush on the same boundary, in the same frame.
fn until_shared_tick() -> std::time::Duration {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    const TICK: u128 = 50_000;
    let micros = START.get_or_init(Instant::now).elapsed().as_micros();
    std::time::Duration::from_micros((TICK - micros % TICK) as u64)
}
