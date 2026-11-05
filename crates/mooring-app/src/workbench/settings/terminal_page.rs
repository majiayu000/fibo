//! 设置 › 终端: the live preview, type size, fonts and the rest of the terminal's rows.
use super::*;

impl SettingsView {
    /// A selectable tile: a picture above, its name below.
    pub(super) fn set_text(&mut self, text: TerminalText, cx: &mut Context<Self>) {
        self.text_error = terminal_text::update(text).err();
        cx.refresh_windows();
        cx.notify();
    }
    /// A segmented control over one terminal choice; `pick` writes it.
    pub(super) fn text_choices<V: Copy + 'static>(
        &self,
        id: &'static str,
        options: &[(V, &'static str)],
        on: impl Fn(V) -> bool,
        pick: fn(&mut TerminalText, V),
        cx: &Context<Self>,
    ) -> gpui::Div {
        let mut choices = segments();
        for (i, &(value, name)) in options.iter().enumerate() {
            choices = choices.child(segment(format!("{id}-{i}"), name, on(value)).on_click(
                cx.listener(move |this, _, _, cx| {
                    let mut text = terminal_text::current();
                    pick(&mut text, value);
                    this.set_text(text, cx);
                }),
            ));
        }
        choices
    }
    fn preview(&self, text: &TerminalText) -> gpui::Div {
        let dot = |hue: u32| div().size(px(T::S10)).rounded_full().bg(T::color(hue));
        div()
            .flex()
            .flex_col()
            .rounded(px(T::CARD_RADIUS))
            .overflow_hidden()
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .bg(T::color(T::terminal_bg()))
            .child(
                div()
                    .h(px(T::PANE_HEADER - T::S4))
                    .px(px(T::S12))
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .border_b(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    // Window buttons, not states.
                    .child(dot(Tone::Danger.hue()))
                    .child(dot(Tone::Warn.hue()))
                    .child(dot(Tone::Ok.hue()))
                    .child(div().flex_1())
                    .child(label(format!(
                        "{} · {} pt · {}×",
                        if text.family.is_empty() {
                            "JetBrains Mono"
                        } else {
                            &text.family
                        },
                        text.size,
                        text.line_height
                    ))),
            )
            .child(
                div().p(px(T::S16)).child(
                    gpui::canvas(
                        |_, _, _| (),
                        |bounds, _, window, cx| {
                            use crate::terminal::screen::{self, Run};
                            let font = T::terminal_mono(cx);
                            let Some(metrics) = screen::CellMetrics::measure(&font, window) else {
                                return;
                            };
                            let fg = T::terminal_fg();
                            // The sample prompt's ANSI green and blue, not states.
                            let green = Tone::Ok.hue();
                            let blue = T::PREVIEW_ANSI_BLUE;
                            let quiet = mix(T::terminal_bg(), fg, 550);
                            let line = |parts: &[(&str, u32)]| {
                                let mut col = 0;
                                parts
                                    .iter()
                                    .map(|&(text, color)| {
                                        let cols = text
                                            .chars()
                                            .map(|c| if c.is_ascii() { 1 } else { 2 })
                                            .sum();
                                        let run = Run {
                                            col,
                                            cols,
                                            text: text.into(),
                                            fg: color,
                                            bg: None,
                                            attrs: screen::Attrs::default(),
                                        };
                                        col += cols;
                                        run
                                    })
                                    .collect::<Vec<_>>()
                            };
                            let preview = screen::Screen {
                                background: T::terminal_bg(),
                                rows: vec![
                                    line(&[
                                        ("~/workspace", blue),
                                        (" on ", quiet),
                                        ("main", green),
                                    ]),
                                    line(&[
                                        ("$ ", quiet),
                                        ("codex", fg),
                                        (" resume --last", quiet),
                                    ]),
                                    line(&[("▸ 准备继续你的工作", fg), ("  0123456789", quiet)]),
                                    line(&[("✓ ", green), ("3 files changed, 0 errors", fg)]),
                                ],
                                ..Default::default()
                            };
                            screen::paint_cached(
                                &preview,
                                &font,
                                bounds,
                                metrics,
                                screen::CursorPaint::Hidden,
                                &mut screen::LayoutCache::default(),
                                window,
                                cx,
                            );
                        },
                    )
                    .w_full()
                    .h(px(text.size * text.line_height * 4.6)),
                ),
            )
    }
    pub(super) fn terminal_page(&self, cx: &Context<Self>) -> gpui::Div {
        let text = terminal_text::current();
        let shows = |title| self.shows(Page::Terminal, title, cx);
        let mut page = div().flex().flex_col();
        if shows("终端字体") || shows("字号") || shows("行距") {
            page = page.child(heading("预览")).child(self.preview(&text));
        }
        let mut size = Vec::new();
        if shows("字号") {
            let stepper = segments()
                .child(segment("font-smaller", "−", false).on_click(cx.listener(
                    |this, _, _, cx| {
                        let mut text = terminal_text::current();
                        text.size = (text.size - 1.).max(8.);
                        this.set_text(text, cx);
                    },
                )))
                .child(
                    div()
                        .w(px(RESET_WIDTH))
                        .text_center()
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::primary()))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(format!("{} pt", text.size)),
                )
                .child(segment("font-larger", "+", false).on_click(cx.listener(
                    |this, _, _, cx| {
                        let mut text = terminal_text::current();
                        text.size = (text.size + 1.).min(36.);
                        this.set_text(text, cx);
                    },
                )));
            size.push(
                self.row(
                    size.is_empty(),
                    "字号",
                    "与 ⌘+ / ⌘− 缩放共用，重开应用后保留",
                    stepper,
                )
                .into_any_element(),
            );
        }
        if shows("行距") {
            let line_height = text.line_height;
            let choices = self.text_choices(
                "line",
                &[
                    (1., "紧凑 1.0×"),
                    (1.2, "标准 1.2×"),
                    (1.4, "舒适 1.4×"),
                    (1.6, "宽松 1.6×"),
                ],
                |h: f32| (line_height - h).abs() < 0.01,
                |t, h| t.line_height = h,
                cx,
            );
            size.push(
                self.row(
                    size.is_empty(),
                    "行距",
                    "相对字体自然行高，文字始终居中",
                    choices,
                )
                .into_any_element(),
            );
        }
        let mut font = Vec::new();
        if shows("终端字体") {
            font.push(self.font_list(&text, cx).into_any_element());
        }
        font.extend(self.face_rows(font.is_empty(), &text, cx));
        let cursor = self.cursor_rows(&text, cx);
        let reading = self.reading_rows(&text, cx);
        let frame = self.frame_rows(&text, cx);
        let mut keys = Vec::new();
        if shows("Option 键") {
            let option = text.option_key;
            let choices = self.text_choices(
                "option-key",
                &[
                    (OptionKey::Auto, "自动"),
                    (OptionKey::Alt, "当 Alt"),
                    (OptionKey::Text, "打字符"),
                ],
                |o| o == option,
                |t, o| t.option_key = o,
                cx,
            );
            keys.push(
                self.row(
                    true,
                    "Option 键",
                    "自动：键盘在这里有 @ [ | 这类符号时打出来，其余当 Alt；要打 ą é 这类字母选「打字符」",
                    choices,
                )
                .into_any_element(),
            );
        }
        let mut paths = Vec::new();
        if shows("用哪个编辑器打开行号") {
            let editor = text.editor;
            // Only the editors on this Mac, besides the automatic and fallback choices.
            let options: Vec<(Editor, &'static str)> = [
                Editor::Auto,
                Editor::Zed,
                Editor::VsCode,
                Editor::Cursor,
                Editor::Terminal,
                Editor::System,
            ]
            .into_iter()
            .filter(|&e| {
                e == editor
                    || !matches!(e, Editor::Zed | Editor::VsCode | Editor::Cursor)
                    || crate::terminal::open_path::installed(e).is_some()
            })
            .map(|e| (e, crate::terminal::open_path::editor_name(e)))
            .collect();
            let choices =
                self.text_choices("editor", &options, |e| e == editor, |t, e| t.editor = e, cx);
            paths.push(
                self.row(
                    true,
                    "用哪个编辑器打开行号",
                    "⌘ 点击 pane.rs:651 这类路径跳到那一行；自动选 Zed、VS Code、Cursor 中第一个装了的",
                    choices,
                )
                .into_any_element(),
            );
        }
        let typing = !size.is_empty() || !font.is_empty();
        page.child(self.cards(vec![
            ("文字大小", size),
            ("终端字体", font),
            ("光标与分屏", cursor),
            ("可读性", reading),
            ("边距", frame),
            ("键盘", keys),
            ("点击路径", paths),
        ]))
        .when(typing, |d| {
            d.child(
                div()
                    .pt(px(T::S12))
                    .flex()
                    .items_center()
                    .child(label("只列出本机已安装的等宽字体；缺的字自动回退").flex_1())
                    .child(secondary("reset-text", "恢复文字默认值").on_click(
                        // Only the type: the cursor and split choices stay as they were.
                        cx.listener(|this, _, _, cx| {
                            let fresh = TerminalText::default();
                            let mut text = terminal_text::current();
                            text.family = fresh.family;
                            text.size = fresh.size;
                            text.line_height = fresh.line_height;
                            text.non_ascii_family = fresh.non_ascii_family;
                            text.ligatures = fresh.ligatures;
                            this.set_text(text, cx);
                        }),
                    )),
            )
        })
    }
}
