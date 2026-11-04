//! 设置 › 终端's rows past the type's size: faces, the cursor, how readable text is, and
//! the margin. All of it is the user's own habit, so changing theme leaves it alone.
use super::*;
use crate::ui::terminal_text::CursorDefault;

/// Faces offered for text past ASCII; only the installed ones show.
const NON_ASCII_FACES: [(&str, &str); 7] = [
    ("", "跟随终端字体"),
    ("PingFang SC", "苹方"),
    ("Hiragino Sans GB", "冬青黑体"),
    ("Heiti SC", "黑体"),
    ("Songti SC", "宋体"),
    ("Kaiti SC", "楷体"),
    ("LXGW WenKai", "霞鹜文楷"),
];

impl SettingsView {
    pub(super) fn font_list(&self, text: &TerminalText, cx: &Context<Self>) -> gpui::Div {
        let installed = cx.text_system().all_font_names();
        let mut list = div().flex().flex_col();
        for (i, family) in MONO_FACES
            .into_iter()
            .filter(|f| f.is_empty() || installed.iter().any(|n| n == f))
            .enumerate()
        {
            let on = text.family == family;
            let name = if family.is_empty() {
                "JetBrains Mono"
            } else {
                family
            };
            list = list.child(
                div()
                    .id(format!("font-{family}"))
                    .role(gpui::accesskit::Role::RadioButton)
                    .aria_label(name)
                    .h(px(T::TODO_HEIGHT + T::S2))
                    .px(px(T::S16))
                    .flex()
                    .items_center()
                    .gap(px(T::S12))
                    .cursor_pointer()
                    .when(i > 0, |d| {
                        d.border_t(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::border()))
                    })
                    .map(|d| {
                        if on {
                            d.bg(T::color(T::raised()))
                        } else {
                            d.hover(|s| s.bg(T::color(T::button_hover())))
                        }
                    })
                    .child(
                        div()
                            .w(px(T::S12))
                            .text_size(px(T::SMALL))
                            .text_color(T::color(T::primary()))
                            .child(if on { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(T::S8))
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::primary()))
                            .when(on, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                            .child(name)
                            .when(family.is_empty(), |d| d.child(label("内置 · 默认"))),
                    )
                    .child(
                        div()
                            .font_family(name)
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::secondary()))
                            .child("Aa 0O il1 => {}"),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let mut text = terminal_text::current();
                        text.family = family.into();
                        this.set_text(text, cx);
                    })),
            );
        }
        list
    }
    /// 中文和符号字体 and 连字, under the font list.
    pub(super) fn face_rows(
        &self,
        text: &TerminalText,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut rows = Vec::new();
        if self.shows(Page::Terminal, "中文和符号字体", cx) {
            let installed = cx.text_system().all_font_names();
            let on = text.non_ascii_family.clone();
            let mut choices = segments();
            for (i, (family, name)) in NON_ASCII_FACES
                .into_iter()
                .filter(|(f, _)| f.is_empty() || installed.iter().any(|n| n == f))
                .enumerate()
            {
                choices = choices.child(
                    segment(format!("non-ascii-{i}"), name, on == family).on_click(cx.listener(
                        move |this, _, _, cx| {
                            let mut text = terminal_text::current();
                            text.non_ascii_family = family.into();
                            this.set_text(text, cx);
                        },
                    )),
                );
            }
            rows.push(
                self.row(
                    false,
                    "中文和符号字体",
                    "英文和数字仍用上面的字体；这里缺的字继续自动回退",
                    choices,
                )
                .into_any_element(),
            );
        }
        if self.shows(Page::Terminal, "连字", cx) {
            let on = text.ligatures;
            let choices = self.text_choices(
                "ligatures",
                &[(false, "关"), (true, "开")],
                |v| v == on,
                |t, v| t.ligatures = v,
                cx,
            );
            rows.push(
                self.row(
                    false,
                    "连字",
                    "字体支持时把 => != 画成一个符号；开着时画面变化多的终端会多花一点时间排版",
                    choices,
                )
                .into_any_element(),
            );
        }
        rows
    }
    pub(super) fn cursor_rows(
        &self,
        text: &TerminalText,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let shows = |title| self.shows(Page::Terminal, title, cx);
        let mut cursor = Vec::new();
        if shows("光标闪烁") {
            let blink = text.cursor_blink;
            let choices = self.text_choices(
                "blink",
                &[(true, "闪烁"), (false, "常亮")],
                |b| b == blink,
                |t, b| t.cursor_blink = b,
                cx,
            );
            cursor.push(
                self.row(
                    cursor.is_empty(),
                    "光标闪烁",
                    "当前分屏的光标；程序要求常亮时照办，打字时保持亮着",
                    choices,
                )
                .into_any_element(),
            );
        }
        if shows("非当前分屏变暗") {
            let dim = text.unfocused_dim;
            let choices = self.text_choices(
                "dim",
                &[(0., "不变暗"), (0.15, "轻"), (0.3, "明显")],
                |d: f32| (dim - d).abs() < 0.01,
                |t, d| t.unfocused_dim = d,
                cx,
            );
            cursor.push(
                self.row(
                    cursor.is_empty(),
                    "非当前分屏变暗",
                    "一个标签里有多个分屏时，没有焦点的分屏淡一些",
                    choices,
                )
                .into_any_element(),
            );
        }
        if shows("非当前分屏的光标") {
            let shape = text.unfocused_cursor;
            let choices = self.text_choices(
                "unfocused-cursor",
                &[
                    (UnfocusedCursor::Hidden, "隐藏"),
                    (UnfocusedCursor::Hollow, "空心框"),
                ],
                |s| s == shape,
                |t, s| t.unfocused_cursor = s,
                cx,
            );
            cursor.push(
                self.row(
                    cursor.is_empty(),
                    "非当前分屏的光标",
                    "隐藏时整个窗口只有一个光标；空心框能看出各分屏输入到哪",
                    choices,
                )
                .into_any_element(),
            );
        }
        if shows("默认光标形状") {
            let on = text.cursor_shape;
            let choices = self.text_choices(
                "cursor-shape",
                &[
                    (CursorDefault::Block, "方块"),
                    (CursorDefault::Bar, "竖线"),
                    (CursorDefault::Underline, "下划线"),
                ],
                |v| v == on,
                |t, v| t.cursor_shape = v,
                cx,
            );
            cursor.push(
                self.row(
                    cursor.is_empty(),
                    "默认光标形状",
                    "程序自己要求某种形状时照办（如 vim 的插入模式）",
                    choices,
                )
                .into_any_element(),
            );
        }
        if shows("光标所在行") {
            let on = text.cursor_guide;
            let choices = self.text_choices(
                "cursor-guide",
                &[(false, "不高亮"), (true, "高亮")],
                |v| v == on,
                |t, v| t.cursor_guide = v,
                cx,
            );
            cursor.push(
                self.row(
                    cursor.is_empty(),
                    "光标所在行",
                    "当前终端的光标行铺一条浅色，颜色可在主题里改",
                    choices,
                )
                .into_any_element(),
            );
        }
        if shows("后台窗口变暗") {
            let dim = text.window_dim;
            let choices = self.text_choices(
                "window-dim",
                &[(0., "不变暗"), (0.15, "轻"), (0.3, "明显")],
                |d: f32| (dim - d).abs() < 0.01,
                |t, d| t.window_dim = d,
                cx,
            );
            cursor.push(
                self.row(
                    cursor.is_empty(),
                    "后台窗口变暗",
                    "切到别的 App 时，终端内容淡一些",
                    choices,
                )
                .into_any_element(),
            );
        }
        cursor
    }
    pub(super) fn reading_rows(
        &self,
        text: &TerminalText,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut rows = Vec::new();
        if self.shows(Page::Terminal, "最低对比度", cx) {
            let on = text.min_contrast;
            let choices = self.text_choices(
                "min-contrast",
                &[(1., "不调整"), (3., "3:1"), (4.5, "4.5:1"), (7., "7:1")],
                |v: f32| (on - v).abs() < 0.01,
                |t, v| t.min_contrast = v,
                cx,
            );
            rows.push(
                self.row(
                    rows.is_empty(),
                    "最低对比度",
                    "程序写出的文字和底色太接近时，把文字往黑或白挪到这个对比度",
                    choices,
                )
                .into_any_element(),
            );
        }
        if self.shows(Page::Terminal, "弱化文字", cx) {
            let on = text.faint;
            let choices = self.text_choices(
                "faint",
                &[(0.35, "更淡"), (0.5, "默认"), (0.7, "更清楚")],
                |v: f32| (on - v).abs() < 0.01,
                |t, v| t.faint = v,
                cx,
            );
            rows.push(
                self.row(
                    rows.is_empty(),
                    "弱化文字",
                    "程序标成弱化（SGR 2）的文字有多淡，比如 Agent 的提示和思考过程",
                    choices,
                )
                .into_any_element(),
            );
        }
        rows
    }
    pub(super) fn frame_rows(
        &self,
        text: &TerminalText,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        if !self.shows(Page::Terminal, "终端边距", cx) {
            return vec![];
        }
        let on = text.padding;
        let choices = self.text_choices(
            "padding",
            &[(0., "无"), (4., "小"), (8., "中"), (16., "大")],
            |v: f32| (on - v).abs() < 0.01,
            |t, v| t.padding = v,
            cx,
        );
        vec![
            self.row(
                true,
                "终端边距",
                "文字和窗格边缘之间的空白；改了之后终端的行列数会跟着变",
                choices,
            )
            .into_any_element(),
        ]
    }
}
