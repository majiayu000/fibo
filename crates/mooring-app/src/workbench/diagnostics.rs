//! An independently updated, disposable view of a short host measurement.
use super::*;
use futures::{
    StreamExt,
    channel::mpsc::{UnboundedSender, unbounded},
};
use mooring_core::diagnostics::{AppUsage, Diagnostics, Memory, Pressure, gib, mib};
use std::{
    io,
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

mod charts;
mod cpu;
mod memory;
mod scatter;
mod view;

/// The host reports its first interval this long after it is asked (its INTERVAL is 7.5 s,
/// plus the first snapshot).
const FIRST_REPORT: Duration = Duration::from_secs(8);
const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
/// One turn of the waiting spinner.
const SPIN: Duration = Duration::from_millis(800);
static SPINNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) enum DiagnosticAction {
    Close,
    Terminal(String),
}

/// Dropping a view (or pressing Stop) shuts its socket, including a pending connect.
struct Sampling {
    socket: Arc<Mutex<Option<UnixStream>>>,
    cancelled: Arc<AtomicBool>,
    /// The host answered the handshake and was asked to sample.
    requested: Arc<AtomicBool>,
    started: Instant,
}
impl Drop for Sampling {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(stream) = self.socket.lock().unwrap().take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}
impl Sampling {
    fn start(path: PathBuf, tx: UnboundedSender<io::Result<Diagnostics>>) -> Self {
        let result = Self {
            socket: Arc::new(Mutex::new(None)),
            cancelled: Arc::new(AtomicBool::new(false)),
            requested: Arc::new(AtomicBool::new(false)),
            started: Instant::now(),
        };
        let socket = result.socket.clone();
        let cancelled = result.cancelled.clone();
        let requested = result.requested.clone();
        std::thread::spawn(move || {
            let read = || -> io::Result<()> {
                let mut stream = UnixStream::connect(path)?;
                {
                    let mut slot = socket.lock().unwrap();
                    if cancelled.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    *slot = Some(stream.try_clone()?);
                }
                stream.set_read_timeout(Some(Duration::from_secs(25)))?;
                stream.set_write_timeout(Some(Duration::from_secs(5)))?;
                send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
                if !matches!(
                    receive::<Reply>(&mut stream)?,
                    Reply::Hello { major: MAJOR, .. }
                ) {
                    return Err(io::Error::other("诊断连接握手失败"));
                }
                send(&mut stream, CONTROL, &Request::Diagnose)?;
                requested.store(true, Ordering::Release);
                loop {
                    let report = match receive::<Reply>(&mut stream)? {
                        Reply::Diagnostics(report) => report,
                        Reply::Error(error) => return Err(io::Error::other(error)),
                        _ => return Err(io::Error::other("诊断响应无效")),
                    };
                    let complete = report.complete;
                    if tx.unbounded_send(Ok(report)).is_err() || complete {
                        return Ok(());
                    }
                }
            };
            if let Err(error) = read()
                && !cancelled.load(Ordering::Acquire)
            {
                let _ = tx.unbounded_send(Err(error));
            }
        });
        result
    }
}

pub(super) struct DiagnosticView {
    socket: PathBuf,
    sampling: Option<Sampling>,
    report: Option<Diagnostics>,
    error: Option<String>,
    copied: bool,
    /// Workbench terminal id → the title the summary and the chips show.
    terminals: HashMap<String, String>,
    /// The app whose terminal chips are expanded.
    open: Option<String>,
    all_rows: bool,
    /// Apps whose terminal rows under the memory ranking are folded; open by default.
    memory_folded: HashSet<String>,
    all_memory_rows: bool,
    notes_open: bool,
    focus: gpui::FocusHandle,
}
impl gpui::EventEmitter<DiagnosticAction> for DiagnosticView {}

/// Rows the CPU ranking shows before「再显示」.
const FIRST_ROWS: usize = 10;

/// Where each scatter label's text starts, in pixels from the plot's top. A label sits
/// centred on its point; one that would overlap a label above it on the same side of the
/// plot moves down a line, and none runs past the bottom edge onto the x-axis labels.
fn label_tops(points: &[(&str, f32, f32)]) -> HashMap<String, f32> {
    let line = T::TICK_ROW;
    let mut order: Vec<_> = points.to_vec();
    order.sort_by(|a, b| a.2.total_cmp(&b.2));
    let mut placed: Vec<(f32, f32)> = Vec::new();
    let mut names = Vec::new();
    let mut tops = HashMap::new();
    for (name, x, y) in order {
        let mut top = y * T::SCATTER_HEIGHT - line / 2.;
        for (px_, ptop) in &placed {
            if (x - px_).abs() < 0.45 && top < ptop + line {
                top = ptop + line;
            }
        }
        placed.push((x, top));
        names.push(name);
    }
    // Clamped to the bottom edge, labels may stack there: push those back up.
    let mut floor_of: Vec<(f32, f32)> = Vec::new();
    for i in (0..placed.len()).rev() {
        let (x, mut top) = placed[i];
        top = top.min(T::SCATTER_HEIGHT - line);
        for (fx, ftop) in &floor_of {
            if (x - fx).abs() < 0.45 && top > ftop - line {
                top = ftop - line;
            }
        }
        top = top.max(0.);
        floor_of.push((x, top));
        tops.insert(names[i].to_owned(), top);
    }
    tops
}

/// Rows the ranking shows before 「再显示」: as many as the right column (scatter plus
/// GPU rows) is tall, so the left side does not end in a blank block; never fewer than 10.
fn first_rows(gpu_rows: usize) -> usize {
    // Scatter, its x labels, the GPU heading, axis and footnote, in ranking rows.
    let fixed = ((T::SCATTER_HEIGHT + T::S20) / T::ROW) as usize + 3;
    FIRST_ROWS.max(fixed + gpu_rows)
}
/// Apps that get their own chart color; the rest share one.
const COLORED: usize = 5;

/// A hover card: a bold first line, then quieter detail lines.
struct Tip(Vec<gpui::SharedString>);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut card = div()
            .max_w(px(T::TIP_WIDTH))
            .px(px(T::S10))
            .py(px(T::S8))
            .rounded(px(T::CONTROL_RADIUS))
            .bg(T::color(T::primary()))
            .text_color(T::color(T::popover()))
            .text_size(px(T::SMALL))
            .font_features(tabular())
            .shadow_lg();
        for (i, line) in self.0.iter().enumerate() {
            card = card.child(if i == 0 {
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(line.clone())
            } else {
                div().child(line.clone())
            });
        }
        card
    }
}
pub(super) fn tip(
    lines: Vec<String>,
) -> impl Fn(&mut Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let lines: Vec<gpui::SharedString> = lines.into_iter().map(Into::into).collect();
    move |_, cx| cx.new(|_| Tip(lines.clone())).into()
}
/// Digits of equal width, so columns of numbers line up.
pub(super) fn tabular() -> gpui::FontFeatures {
    gpui::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]))
}
fn cores(percent: f64) -> String {
    if percent < 100. {
        format!("{:.2}", percent / 100.)
    } else {
        format!("{:.1}", percent / 100.)
    }
}
const GB: f64 = 1_073_741_824.;
/// Apps keep the color the CPU ranking gave them in every chart; others share one gray.
fn app_colors(report: &Diagnostics) -> HashMap<String, u32> {
    report
        .ranked()
        .take(COLORED)
        .enumerate()
        .map(|(i, a)| (a.name.clone(), T::series(i)))
        .collect()
}

impl DiagnosticView {
    fn new(socket: PathBuf, terminals: HashMap<String, String>, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            socket,
            sampling: None,
            report: None,
            error: None,
            copied: false,
            terminals,
            open: None,
            all_rows: false,
            memory_folded: HashSet::new(),
            all_memory_rows: false,
            notes_open: false,
            focus: cx.focus_handle(),
        };
        view.start(cx);
        view
    }
    fn start(&mut self, cx: &mut Context<Self>) {
        self.report = None;
        self.error = None;
        self.copied = false;
        let (tx, mut rx) = unbounded();
        let sampling = Sampling::start(self.socket.clone(), tx);
        let cancelled = sampling.cancelled.clone();
        self.sampling = Some(sampling);
        cx.spawn(async move |this, cx| {
            while let Some(result) = rx.next().await {
                // A buffered result from a stopped run must not replace a newer run.
                if cancelled.load(Ordering::Acquire) {
                    break;
                }
                if this
                    .update(cx, |this, cx| {
                        match result {
                            Ok(report) => {
                                if report.complete {
                                    this.sampling = None;
                                }
                                this.report = Some(report);
                            }
                            Err(error) => {
                                this.error = Some(format!("采样未完成：{error}"));
                                this.sampling = None;
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    /// What the host is doing before its first report, as terminal lines that appear as
    /// each step is reached. Lines do not fade in: a frame drawn without its predecessor
    /// would restart the fade and show nothing. The spinner's animation also redraws the view every frame,
    /// so the lines and the countdowns follow the clock without a timer.
    fn waiting(&self, sampling: &Sampling, waited: Duration, cx: &Context<Self>) -> gpui::Div {
        let requested = sampling.requested.load(Ordering::Acquire);
        let line = |id: &'static str, mark: gpui::AnyElement, text: gpui::AnyElement| {
            div()
                .flex()
                .items_center()
                .child(div().w(px(T::S20)).flex_shrink_0().child(mark))
                .child(text)
                .id(id)
                .into_any_element()
        };
        // One redraw per glyph (100 ms), not per display frame: the panel measures the
        // CPU of every app, Mooring included, and a 120 Hz spinner inflated its own bar.
        super::pace::wake(&SPINNING, SPIN / SPINNER.len() as u32, cx);
        let spinner = || {
            let t = super::pace::phase(SPIN);
            let i = ((t * SPINNER.len() as f32) as usize).min(SPINNER.len() - 1);
            div()
                .text_color(T::color(T::primary()))
                .child(SPINNER[i])
                .into_any_element()
        };
        let done = || div().child("✓").into_any_element();
        let mut lines = div()
            .p(px(T::S20))
            .flex()
            .flex_col()
            .gap(px(T::S6))
            .font_family(T::mono(cx).family)
            .text_size(px(T::MONO_SIZE))
            .text_color(T::color(T::secondary()));
        if !requested {
            return lines.child(line(
                "diagnostic-connect",
                spinner(),
                div()
                    .text_color(T::color(T::primary()))
                    .child("连接 Mooring 后台…")
                    .into_any_element(),
            ));
        }
        let left = FIRST_REPORT.saturating_sub(waited).as_secs_f32().ceil();
        lines = lines
            .child(line(
                "diagnostic-connected",
                done(),
                div().child("连上后台").into_any_element(),
            ))
            .child(line(
                "diagnostic-first",
                done(),
                div()
                    .child("后台记下第一次快照：进程、CPU 时间、GPU 计数")
                    .into_any_element(),
            ))
            .child(line(
                "diagnostic-second",
                spinner(),
                div()
                    .flex()
                    .gap(px(T::S6))
                    .child(
                        div()
                            .text_color(T::color(T::primary()))
                            .child("隔几秒再记一次，用两次的差算占用"),
                    )
                    .child(if left > 0. {
                        format!("· 还剩 {left} 秒")
                    } else {
                        "· 马上就好".into()
                    })
                    .into_any_element(),
            ));
        lines
    }
    fn summary(&self) -> Option<String> {
        let report = self.report.as_ref()?;
        Some(report.summary(&clock(report.started_at), &self.terminals))
    }
    fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.summary() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            self.copied = true;
            cx.notify();
        }
    }
}

impl Workbench {
    #[cfg(feature = "visual-test")]
    pub fn visual_open_diagnostics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_diagnostics(window, cx);
    }
    #[cfg(feature = "visual-test")]
    pub fn visual_diagnostic_report(&self, cx: &gpui::App) -> Option<Diagnostics> {
        self.diagnostics.as_ref()?.read(cx).report.clone()
    }
    /// Pretends memory is tight, so the screenshot shows the warn colors and order.
    #[cfg(feature = "visual-test")]
    pub fn visual_tighten_diagnostic_memory(&mut self, cx: &mut Context<Self>) {
        super::pressure::force(2);
        cx.notify();
        let view = self.diagnostics.as_ref().unwrap().clone();
        view.update(cx, |view, cx| {
            if let Some(m) = view.report.as_mut().and_then(|r| r.memory.as_mut()) {
                m.pressure = Some(Pressure::Warn);
                m.swap_bytes = 3 << 30;
            }
            cx.notify();
        });
    }
    #[cfg(feature = "visual-test")]
    pub fn visual_diagnostic_closed(&self) -> bool {
        self.diagnostics.is_none()
    }
    #[cfg(feature = "visual-test")]
    pub fn visual_open_diagnostic_terminal(&mut self, cx: &mut Context<Self>) {
        let view = self.diagnostics.as_ref().unwrap().clone();
        let terminal = view
            .read(cx)
            .report
            .as_ref()
            .unwrap()
            .apps
            .iter()
            .flat_map(|app| &app.terminals)
            .next()
            .expect("no managed terminal in native report")
            .clone();
        view.update(cx, |_, cx| cx.emit(DiagnosticAction::Terminal(terminal)));
    }
    #[cfg(feature = "visual-test")]
    pub fn visual_copy_diagnostics(&mut self, cx: &mut Context<Self>) {
        let original = cx.read_from_clipboard();
        let view = self.diagnostics.as_ref().unwrap().clone();
        let expected = view.read(cx).summary().unwrap();
        view.update(cx, |view, cx| view.copy(cx));
        let copied = cx.read_from_clipboard().and_then(|item| item.text());
        if let Some(original) = original {
            cx.write_to_clipboard(original);
        }
        assert_eq!(copied.as_deref(), Some(expected.as_str()));
    }

    pub(crate) fn show_diagnostics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.hide();
        let terminals = self
            .workspace
            .terminals
            .iter()
            .map(|t| {
                let title = self
                    .workspace
                    .tabs
                    .iter()
                    .find(|tab| tab.layout.as_ref().is_some_and(|l| l.contains(&t.id)))
                    .map(|tab| tab.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| home_relative(&t.cwd));
                (t.id.clone(), title)
            })
            .collect();
        let view = cx.new(|cx| DiagnosticView::new(self.store.socket().to_owned(), terminals, cx));
        cx.subscribe(&view, |this, _, action, cx| {
            this.diagnostics = None;
            this.nav.pending_focus = true;
            if let DiagnosticAction::Terminal(id) = action
                && let Some(tab) = this
                    .workspace
                    .tabs
                    .iter()
                    .find(|tab| tab.layout.as_ref().is_some_and(|l| l.contains(id)))
            {
                this.nav.space = tab.space.clone();
                this.nav.tab = Some(tab.id.clone());
                this.nav.focused = Some(id.clone());
                this.nav.overview = false;
                this.nav.task_detail = None;
                this.nav.zoomed = None;
                this.tasks.form = None;
                this.overview.cleanup.clear();
                this.sync_panes(cx);
            }
            cx.notify();
        })
        .detach();
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.diagnostics = Some(view);
        cx.notify();
    }
    pub(super) fn diagnostic_overlay(
        &self,
        view: Entity<DiagnosticView>,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .justify_center()
                .items_center()
                .p(px(T::S20))
                .bg(gpui::rgba(T::scrim()))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.diagnostics = None;
                        this.nav.pending_focus = true;
                        cx.notify();
                    }),
                )
                .child(view),
        )
        .with_priority(2)
        .into_any_element()
    }
}

#[cfg(test)]
mod row_tests;
