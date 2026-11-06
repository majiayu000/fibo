//! 看改动: this turn's net changes, read only, over part of the terminal. It covers the
//! terminal rather than pushing it down, so the program under it is not resized, and leaves
//! the Agent's last words and the input line showing, so you can answer as you read.
//!
//! It is a cached view of its own and lays out only the rows on screen: the window redraws
//! for any pane's output, and this should cost nothing when it has not changed. What it
//! shows is asked of the host when it opens and is never in the workspace snapshot.
use super::*;
use crate::terminal::pane::{Hovered, Pane};
use crate::ui::tone::Tone;
use mooring_core::turn_diff::{Disk, LineKind, TurnChanges, TurnFile};
use std::sync::Arc;

/// How much of the terminal it covers.
pub(super) const COVERS: f32 = 0.56;
const ROW_HEIGHT: f32 = 19.;
/// The numbers' columns.
const GUTTER: f32 = 44.;
const SIGN: f32 = 16.;
const LIST_WIDTH: f32 = 236.;
const LIST_LEAST: f32 = 140.;
/// An arrow key's scroll.
const STEP: f32 = 60.;
/// A read shorter than this shows no 「正在读取」, which would only flash.
const READING_AFTER: Duration = Duration::from_millis(150);

pub(super) enum Load {
    Reading { say: bool },
    Failed,
    Ready(TurnChanges),
}

pub(super) enum Row {
    /// Where a stretch of the file starts.
    Gap(u32),
    Line {
        kind: LineKind,
        old: u32,
        new: u32,
        text: gpui::SharedString,
    },
}

/// One pane's panel, kept when closed so reopening returns to the same file and place.
pub(in crate::workbench) struct ChangesSlot {
    /// The turn it shows: another means it is out of date.
    pub(super) key: (u64, u64),
    pub(super) open: bool,
    /// It covers the lower part of the terminal rather than the upper.
    pub(super) below: bool,
    pub(super) view: Entity<ChangesPanel>,
}

pub(in crate::workbench) struct ChangesPanel {
    pub(super) workbench: gpui::WeakEntity<Workbench>,
    pub(super) terminal: String,
    pub(super) session: String,
    pub(super) pane: Entity<Pane>,
    pub(super) focus: gpui::FocusHandle,
    pub(super) load: Load,
    pub(super) selected: usize,
    pub(super) rows: Vec<Arc<Vec<Row>>>,
    /// One per file, so each keeps its own scroll.
    pub(super) lists: Vec<gpui::ListState>,
    #[cfg(feature = "visual-test")]
    pub(super) drew: u32,
}

fn rows_of(file: &TurnFile) -> Vec<Row> {
    let mut rows = Vec::new();
    for hunk in &file.hunks {
        if !file.created {
            let first = hunk.lines.first().map_or(0, |l| l.new.max(l.old));
            rows.push(Row::Gap(first));
        }
        rows.extend(hunk.lines.iter().map(|l| Row::Line {
            kind: l.kind,
            old: l.old,
            new: l.new,
            text: if l.text.is_empty() {
                " ".into()
            } else {
                l.text.replace('\t', "    ").into()
            },
        }));
    }
    rows
}

fn number(n: u32) -> String {
    if n == 0 { String::new() } else { n.to_string() }
}

fn row_element(row: &Row, font: &gpui::Font) -> gpui::AnyElement {
    let base = div()
        .w_full()
        .flex()
        .font(font.clone())
        .text_size(px(T::SMALL))
        .line_height(px(ROW_HEIGHT));
    match row {
        Row::Gap(line) => base
            .px(px(T::S14))
            .bg(T::color(T::chip()))
            .text_color(T::color(T::muted()))
            .child(format!("第 {line} 行"))
            .into_any_element(),
        Row::Line {
            kind,
            old,
            new,
            text,
        } => {
            let (ink, wash, sign) = match kind {
                LineKind::Added => (Tone::Ok.ink(), Some(Tone::Ok.wash()), "+"),
                LineKind::Removed => (Tone::Danger.ink(), Some(Tone::Danger.wash()), "−"),
                LineKind::Same => (T::color(T::secondary()), None, ""),
            };
            let gutter = |n: u32| {
                div()
                    .w(px(GUTTER))
                    .flex_shrink_0()
                    .pr(px(T::S4))
                    .flex()
                    .justify_end()
                    .text_color(T::color(T::muted()))
                    .child(number(n))
            };
            base.when_some(wash, |d, w| d.bg(w))
                .child(gutter(*old))
                .child(gutter(*new))
                .child(
                    div()
                        .w(px(SIGN))
                        .flex_shrink_0()
                        .flex()
                        .justify_center()
                        .text_color(ink)
                        .child(sign),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pr(px(T::S12))
                        .text_color(ink)
                        .child(text.clone()),
                )
                .into_any_element()
        }
    }
}

/// The line under the title: what about this file needs saying.
fn warning(file: &TurnFile) -> Option<&'static str> {
    if file.one_by_one {
        return Some("这个文件中途被别的程序改过，这里按次显示每一次编辑。");
    }
    match file.disk {
        Disk::Same => None,
        Disk::Changed if file.edits > 0 => {
            Some("这是 Agent 当时的改动。文件后来又变了，和磁盘上的内容可能不一样。")
        }
        Disk::Changed => None,
        Disk::Gone if file.edits > 0 => Some("这个文件后来被删了。"),
        Disk::Gone => None,
    }
}

fn empty_words(file: &TurnFile) -> Option<&'static str> {
    if file.edits == 0 {
        Some("这个文件只有行数，没有改动内容")
    } else if file.hunks.is_empty() {
        Some("这一轮改完又改回去了，净改动为零")
    } else {
        None
    }
}

/// `…/layout/turn_bar.rs`: enough of a path to tell files apart in a narrow column.
fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    match parts.as_slice() {
        [name, dir, _] => format!("…/{dir}/{name}"),
        _ => path.to_owned(),
    }
}

impl ChangesPanel {
    /// Asks again, after a failure.
    fn read_again(&mut self, cx: &mut Context<Self>) {
        let request = Request::TurnChanges {
            session: self.session.clone(),
        };
        let asked = self.workbench.read_with(cx, |w, _| w.store.send(request));
        if let Ok(asked) = asked {
            self.read(asked, cx);
        }
    }

    /// `asked` is the request to the host, made by whoever can reach the store from where
    /// they are: the workbench is mid-update when it opens a panel.
    pub(super) fn read(
        &mut self,
        asked: impl std::future::Future<Output = std::io::Result<Reply>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.load = Load::Reading { say: false };
        let read = cx.background_spawn(asked);
        cx.spawn(async move |this, cx| {
            let reply = read.await;
            this.update(cx, |this, cx| {
                this.loaded(reply, cx);
            })
            .ok();
        })
        .detach();
        let wait = cx.background_executor().timer(READING_AFTER);
        cx.spawn(async move |this, cx| {
            wait.await;
            this.update(cx, |this, cx| {
                if let Load::Reading { say } = &mut this.load {
                    *say = true;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn loaded(&mut self, reply: std::io::Result<Reply>, cx: &mut Context<Self>) {
        match reply {
            Ok(Reply::TurnChanges(changes)) => {
                self.rows = (changes.files.iter())
                    .map(|f| Arc::new(rows_of(f)))
                    .collect();
                self.lists = (self.rows.iter())
                    .map(|r| {
                        gpui::ListState::new(r.len(), gpui::ListAlignment::Top, px(ROW_HEIGHT))
                    })
                    .collect();
                self.selected = 0;
                self.load = Load::Ready(changes);
            }
            _ => self.load = Load::Failed,
        }
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let terminal = self.terminal.clone();
        self.workbench
            .update(cx, |w, cx| w.close_changes(&terminal, window, cx))
            .ok();
    }

    fn select(&mut self, file: usize, cx: &mut Context<Self>) {
        if file != self.selected {
            self.selected = file;
            cx.notify();
        }
    }

    fn open_file(&mut self, cx: &mut Context<Self>) {
        let Load::Ready(changes) = &self.load else {
            return;
        };
        let Some(file) = changes.files.get(self.selected) else {
            return;
        };
        let first = (file.hunks.iter().flat_map(|h| &h.lines))
            .find(|l| l.kind == LineKind::Added)
            .map(|l| (l.new, None));
        let target = Hovered {
            path: file.path.clone().into(),
            at: first,
            whole: true,
            cwd: None,
        };
        self.pane
            .update(cx, |pane, cx| pane.open_target(&target, false, cx));
    }

    /// Reading keys are the panel's. Any other key is typing: the keyboard goes back to the
    /// terminal and the key carries on to the input handler below, which is the terminal's,
    /// so a first letter, composed or not, lands there as if it had been focused.
    fn key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        let held = &key.modifiers;
        if held.platform || held.control || held.function {
            return;
        }
        let files = self.lists.len();
        let list = self.lists.get(self.selected);
        match key.key.as_str() {
            "escape" => self.close(window, cx),
            "up" | "down" => {
                let step = if key.key == "up" { -STEP } else { STEP };
                if let Some(list) = list {
                    list.scroll_by(px(step));
                }
                cx.notify();
            }
            "pageup" | "pagedown" => {
                if let Some(list) = list {
                    let page = f32::from(list.viewport_bounds().size.height) - ROW_HEIGHT * 2.;
                    list.scroll_by(px(if key.key == "pageup" { -page } else { page }));
                }
                cx.notify();
            }
            "tab" if files > 1 => {
                let step = if held.shift { files - 1 } else { 1 };
                self.select((self.selected + step) % files, cx);
            }
            // Never sends: the terminal is what sends, and it does not have the keyboard.
            "tab" | "enter" | "backspace" | "delete" | "left" | "right" | "home" | "end" => {}
            _ => {
                let pane = self.pane.read(cx).focus_handle(cx);
                window.focus(&pane, cx);
                return;
            }
        }
        cx.stop_propagation();
    }

    fn title(&self, file: Option<&TurnFile>, cx: &mut Context<Self>) -> gpui::Div {
        let row = div()
            .h(px(T::PANE_HEADER))
            .flex_shrink_0()
            .pl(px(T::S14))
            .pr(px(T::S6))
            .flex()
            .items_center()
            .gap(px(T::S8))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .text_size(px(T::SMALL));
        let row = match file {
            Some(file) => {
                let (dir, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));
                let note = if file.created {
                    Some("新文件".to_owned())
                } else if file.edits > 1 && !file.one_by_one {
                    Some(format!("编辑 {} 次，这里是合并后的结果", file.edits))
                } else {
                    None
                };
                row.child(
                    div()
                        .flex_shrink_0()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(T::color(T::primary()))
                        .child(name.to_owned()),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font(T::mono(cx))
                        .text_color(T::color(T::muted()))
                        .child(super::super::home_relative(dir)),
                )
                .child(line_counts((file.added, file.removed)))
                .children(note.map(|n| {
                    div()
                        .flex_shrink_0()
                        .text_color(T::color(T::muted()))
                        .child(n)
                }))
            }
            None => row.child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(T::primary()))
                    .child("看改动"),
            ),
        };
        row.child(div().flex_1())
            .when(file.is_some(), |d| {
                d.child(
                    button("changes-open-file", "打开文件")
                        .flex_shrink_0()
                        .h(px(T::ICON_HEIGHT))
                        .px(px(T::S6))
                        .text_color(T::color(T::secondary()))
                        .on_click(cx.listener(|this, _, _, cx| this.open_file(cx))),
                )
            })
            .child(
                button("changes-close", "✕")
                    .flex_shrink_0()
                    .h(px(T::ICON_HEIGHT))
                    .w(px(T::ICON_HEIGHT))
                    .px(px(0.))
                    .text_color(T::color(T::muted()))
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
    }

    fn file_list(&self, files: &[TurnFile], cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("changes-files")
            .flex_shrink_0()
            .w(gpui::relative(0.25))
            .min_w(px(LIST_LEAST))
            .max_w(px(LIST_WIDTH))
            .h_full()
            .overflow_y_scroll()
            .py(px(T::S4))
            .border_r(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .children(files.iter().enumerate().map(|(i, file)| {
                let on = i == self.selected;
                div()
                    .id(gpui::SharedString::from(format!("changes-file-{i}")))
                    .h(px(T::PANE_ACTION))
                    .pl(px(T::S14))
                    .pr(px(T::S10))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .cursor_pointer()
                    .text_size(px(T::SMALL))
                    .text_color(T::color(if on { T::primary() } else { T::secondary() }))
                    .when(on, |d| d.bg(T::color(T::selection())))
                    .hover(|d| d.bg(T::color(T::button_hover())))
                    .on_click(cx.listener(move |this, _, _, cx| this.select(i, cx)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font(T::mono(cx))
                            .child(short_path(&file.path)),
                    )
                    .child(line_counts((file.added, file.removed)))
            }))
    }

    fn middle_words(words: impl Into<gpui::SharedString>) -> gpui::Div {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(T::S8))
            .items_center()
            .justify_center()
            .text_size(px(T::SMALL))
            .text_color(T::color(T::secondary()))
            .child(words.into())
    }
}

impl Render for ChangesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "visual-test")]
        {
            self.drew += 1;
        }
        let focused = self.focus.is_focused(window);
        let hint = |keys: &'static str, what: &'static str| {
            div().flex_shrink_0().child(format!("{keys} {what}"))
        };
        let (file, files) = match &self.load {
            Load::Ready(c) => (c.files.get(self.selected).cloned(), c.files.clone()),
            _ => (None, Vec::new()),
        };
        let main: gpui::AnyElement = match &self.load {
            Load::Reading { say } => {
                Self::middle_words(if *say { "正在读取…" } else { "" }).into_any_element()
            }
            Load::Failed => Self::middle_words("读不到这一轮的会话记录")
                .child(
                    button("changes-retry", "再试一次")
                        .h(px(T::PANE_ACTION))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::control()))
                        .on_click(cx.listener(|this, _, _, cx| this.read_again(cx))),
                )
                .into_any_element(),
            Load::Ready(_) => match (&file, file.as_ref().and_then(empty_words)) {
                (None, _) => Self::middle_words("这一轮没有文件改动").into_any_element(),
                (Some(_), Some(words)) => Self::middle_words(words).into_any_element(),
                (Some(_), None) => {
                    let rows = self.rows[self.selected].clone();
                    let font = T::mono(cx);
                    gpui::list(self.lists[self.selected].clone(), move |i, _, _| {
                        row_element(&rows[i], &font)
                    })
                    .size_full()
                    .into_any_element()
                }
            },
        };
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .when(files.len() > 1, |d| d.child(self.file_list(&files, cx)))
            .child(div().flex_1().min_w_0().h_full().child(main));
        let warn = file.as_ref().and_then(warning).map(|words| {
            div()
                .flex_shrink_0()
                .px(px(T::S14))
                .py(px(T::S4))
                .border_b(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .bg(Tone::Warn.wash())
                .text_color(Tone::Warn.ink())
                .text_size(px(T::SMALL))
                .child(words)
        });
        let cut = file.as_ref().filter(|f| f.cut).map(|f| {
            div()
                .flex_shrink_0()
                .px(px(T::S14))
                .py(px(T::S4))
                .border_b(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .bg(Tone::Warn.wash())
                .text_color(Tone::Warn.ink())
                .text_size(px(T::SMALL))
                .child(format!(
                    "太长了，只显示前面一部分（共 +{} −{}）。完整内容请打开文件。",
                    f.added, f.removed
                ))
        });
        let foot = div()
            .h(px(T::FOOTER))
            .flex_shrink_0()
            .px(px(T::S14))
            .flex()
            .items_center()
            .gap(px(T::S12))
            .overflow_hidden()
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .text_size(px(T::PANE_FOOTER_SIZE))
            .text_color(T::color(T::muted()))
            .child(div().flex_shrink_0().child("用命令或脚本改的文件不在这里"))
            .child(div().flex_1())
            .when(files.len() > 1, |d| d.child(hint("Tab", "下一个文件")))
            .child(hint("↑↓", "滚动"))
            .child(hint("Esc", "关闭"));
        let focus = self.focus.clone();
        let pane = self.pane.clone();
        div()
            .id("changes-panel")
            .track_focus(&self.focus)
            .key_context("ChangesPanel")
            .on_key_down(cx.listener(Self::key_down))
            .occlude()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus.clone(), cx);
                    cx.stop_propagation();
                }),
            )
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(T::surface(T::pane()))
            .border_y(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            // Text typed while it has the keyboard is the terminal's.
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            gpui::ElementInputHandler::new(bounds, pane.clone()),
                            cx,
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(self.title(file.as_ref(), cx))
            .children(warn)
            .children(cut)
            .child(body)
            .child(foot)
            .when(focused, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::focus())),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooring_core::turn_diff::{DiffLine, Hunk};

    fn line(kind: LineKind, old: u32, new: u32, text: &str) -> DiffLine {
        DiffLine {
            kind,
            old,
            new,
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_new_file_has_no_stretch_headers_an_edit_does() {
        let hunk = Hunk {
            lines: vec![
                line(LineKind::Removed, 7, 0, "\tx"),
                line(LineKind::Added, 0, 7, ""),
            ],
        };
        let edited = TurnFile {
            hunks: vec![hunk.clone()],
            edits: 1,
            ..Default::default()
        };
        let rows = rows_of(&edited);
        assert!(matches!(rows[0], Row::Gap(7)));
        assert!(matches!(&rows[1], Row::Line { text, .. } if text.as_ref() == "    x"));
        assert!(matches!(&rows[2], Row::Line { text, .. } if text.as_ref() == " "));
        let created = TurnFile {
            created: true,
            ..edited
        };
        assert_eq!(rows_of(&created).len(), 2);
    }

    #[test]
    fn what_a_file_says_when_it_has_nothing_to_list() {
        let none = TurnFile::default();
        assert_eq!(empty_words(&none), Some("这个文件只有行数，没有改动内容"));
        let undone = TurnFile {
            edits: 2,
            ..Default::default()
        };
        assert_eq!(
            empty_words(&undone),
            Some("这一轮改完又改回去了，净改动为零")
        );
    }

    #[test]
    fn a_file_with_no_log_never_warns_about_the_disk() {
        let gone = TurnFile {
            disk: Disk::Gone,
            ..Default::default()
        };
        assert_eq!(warning(&gone), None);
        let edited = TurnFile {
            disk: Disk::Gone,
            edits: 1,
            ..Default::default()
        };
        assert!(warning(&edited).is_some());
    }

    #[test]
    fn short_paths_keep_their_last_two_parts() {
        assert_eq!(short_path("/a/b/c/d.rs"), "…/c/d.rs");
        assert_eq!(short_path("d.rs"), "d.rs");
        assert_eq!(short_path("c/d.rs"), "c/d.rs");
    }
}
