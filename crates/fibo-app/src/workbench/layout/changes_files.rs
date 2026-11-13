//! 看改动's column of files. Only the rows on screen are laid out, so a turn of thousands of
//! files costs what a dozen do; past a few it gains a filter.
use super::changes_panel::ChangesPanel;
use super::changes_rows::short_path;
use super::*;
use fibo_core::turn_diff::TurnChanges;
use std::ops::Range;

const WIDTH: f32 = 236.;
const LEAST: f32 = 140.;

impl ChangesPanel {
    fn file_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let Some(changes) = self.changes().cloned() else {
            return Vec::new();
        };
        let matches = self.matches.clone();
        let font = T::mono(cx);
        let selected = self.selected;
        range
            .filter_map(|at| {
                let i = *matches.get(at)?;
                let file = changes.files.get(i)?;
                let on = i == selected;
                Some(
                    div()
                        .id(("changes-file", i))
                        .w_full()
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
                        .on_click(cx.listener(move |this, _, _, cx| this.show(i, cx)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font(font.clone())
                                .child(short_path(&file.path)),
                        )
                        .child(line_counts((file.added, file.removed)))
                        .into_any_element(),
                )
            })
            .collect()
    }

    /// The filter, when there are enough files to need one, and the list.
    pub(super) fn file_column(&self, changes: &TurnChanges, cx: &mut Context<Self>) -> gpui::Div {
        let total = changes.files.len();
        let shown = self.matches.len();
        let list: gpui::AnyElement = if shown == 0 {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(T::SMALL))
                .text_color(T::color(T::muted()))
                .child(t!("没有匹配的文件"))
                .into_any_element()
        } else {
            gpui::uniform_list(
                "changes-files",
                shown,
                cx.processor(|this, range, _, cx| this.file_rows(range, cx)),
            )
            .track_scroll(&self.file_scroll)
            .flex_1()
            .min_h_0()
            .py(px(T::S4))
            .into_any_element()
        };
        let filter = self.filter.clone().map(|field| {
            div()
                .flex_shrink_0()
                .h(px(T::PANE_HEADER))
                .pl(px(T::S14))
                .pr(px(T::S10))
                .flex()
                .items_center()
                .gap(px(T::S8))
                .border_b(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .text_size(px(T::SMALL))
                .child(div().flex_1().min_w_0().child(field))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(T::color(T::muted()))
                        .font_features(T::tabular())
                        .child(if shown == total {
                            t!("{total} 个", total)
                        } else {
                            format!("{shown} / {total}")
                        }),
                )
        });
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .w(gpui::relative(0.25))
            .min_w(px(LEAST))
            .max_w(px(WIDTH))
            .h_full()
            .border_r(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .children(filter)
            .child(list)
    }
}
