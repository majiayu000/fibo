//! Closing 本机诊断 mid-run does not stop it: the run moves to the status bar's 本机诊断
//! cell, which says how it is going, and a click brings the sheet back as it is. Before,
//! the sheet covered the window for the whole run and closing it threw the run away.
use super::*;
use crate::workbench::statusbar::item;

impl Workbench {
    /// Closes the sheet. A run still sampling goes on in the background; anything else
    /// is dropped, and the next open samples afresh.
    pub(crate) fn hide_diagnostics(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.diagnostics.take()
            && view.read(cx).sampling.is_some()
        {
            self.diagnostics_away = Some(view);
        }
        self.nav.pending_focus = true;
        cx.notify();
    }

    /// The status bar's 本机诊断 cell: the plain entry, or the state of a closed run.
    pub(crate) fn diagnose_item(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let open = cx.listener(|this, _, window, cx| this.show_diagnostics(window, cx));
        let Some(view) = &self.diagnostics_away else {
            return item("diagnose-host", false)
                .aria_label(t!("本机诊断"))
                .child(t!("本机诊断"))
                .on_click(open)
                .into_any_element();
        };
        let view = view.read(cx);
        let (text, dot, tip): (String, gpui::Hsla, String) = if view.sampling.is_some() {
            (
                t!("诊断采样中…").into(),
                T::color(T::secondary()),
                t!("面板关了也在采样，点开看进度").into(),
            )
        } else if view.error.is_some() {
            (
                t!("诊断没采完").into(),
                Tone::Warn.dot(),
                t!("点开看原因，可以重新采样").into(),
            )
        } else {
            (
                t!("诊断好了").into(),
                Tone::Ok.dot(),
                t!("点开看结果").into(),
            )
        };
        item("diagnose-host", false)
            .aria_label(text.clone())
            .gap(px(T::S4))
            .child(div().size(px(T::DOT)).rounded_full().bg(dot))
            .child(text)
            .tooltip(super::tip(vec![tip]))
            .on_click(open)
            .into_any_element()
    }
}
