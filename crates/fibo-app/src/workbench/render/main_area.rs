//! The main column's notices, active page and pane grid.
use super::*;

impl Workbench {
    pub(super) fn main_area(
        &self,
        terminal_style: bool,
        header: gpui::Div,
        page: Option<gpui::Stateful<gpui::Div>>,
        current: Option<&Tab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let mut main = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            // One white surface under the title row and the panes; the sidebar's gray
            // beside it is the only other layer.
            .when(!terminal_style, |d| d.bg(T::surface(T::pane())))
            .map(|d| self.tab_drop_area(d, cx))
            .child(header);
        // Indexing in progress is a top-bar note; only a lost link to the host is worth a
        // strip, in words the user can act on. A missing official status source is not:
        // logs and screens still tell the state. The menubar's failed refresh is the same
        // outage, so while the link is down it only adds to the strip's details.
        let status = cx
            .try_global::<crate::desktop::Status>()
            .map(|s| s.0.clone())
            .filter(|s| !s.is_empty());
        if self.store.cut_off() {
            let mut details: Vec<String> = self
                .store
                .cut_off_reason()
                .map(str::to_owned)
                .into_iter()
                .collect();
            details.extend(status);
            main =
                main.child(self.notice_strip(t!("后台连接断开，正在重新连接…"), details, true, cx));
        } else if let Some(status) = status.filter(|_| {
            // A failed refresh while the link is down for a moment is the same blip.
            !self.store.link_down()
        }) {
            let (text, detail) = status.split_once('：').unwrap_or((&status, ""));
            let details = vec![detail.to_string()];
            main = main.child(self.notice_strip(text, details, false, cx));
        }
        if let Some(strip) = self.error_strip(cx) {
            main = main.child(strip);
        }
        main = main.children(self.quiet_space_strip(cx));
        if let Some(form) = &self.tasks.form {
            main = main.child(self.form_panel(form, window, cx));
        } else if let Some(page) = page {
            main = main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .track_focus(&self.overview.focus)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            if !this.search.is_open() {
                                window.focus(&this.overview.focus, cx);
                            }
                        }),
                    )
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                        let k = &event.keystroke;
                        if !k.modifiers.modified() && this.cleanup_key(&k.key, window, cx) {
                            cx.stop_propagation();
                            return;
                        }
                        // The board's drawer walks its column with j/k or the arrows.
                        if this.overview.drawer.is_some() && !k.modifiers.modified() {
                            let by = match k.key.as_str() {
                                "j" | "down" => 1,
                                "k" | "up" => -1,
                                _ => 0,
                            };
                            if by != 0 {
                                this.step_drawer(by, cx);
                                cx.stop_propagation();
                                return;
                            }
                        }
                        if k.key == "n"
                            && !k.modifiers.modified()
                            && this.nav.task_detail.is_none()
                            && !this.search.is_open()
                            && this.tasks.board_quick.is_none()
                        {
                            this.show_quick_task(window, cx);
                            cx.stop_propagation();
                            return;
                        }
                        if k.key != "escape" || k.modifiers.modified() {
                            return;
                        }
                        // A card's menu closes first, then the drawer; 逐个清理 steps back
                        // to the overview.
                        if this.tasks.picking.take().is_some()
                            || this.overview.card_menu.take().is_some()
                            || this.overview.drawer.take().is_some()
                            || this.nav.task_detail.take().is_some()
                        {
                        } else if !this.overview.cleanup.is_empty() {
                            this.overview.cleanup.clear();
                        } else {
                            this.leave_overview(cx);
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(page),
            );
        } else if let Some(task) = self
            .nav
            .task_detail
            .as_ref()
            .and_then(|id| self.workspace.tasks.iter().find(|t| &t.id == id))
        {
            main = main.child(self.task_detail_view(task, cx));
        } else {
            // No strip for the tab's todo above the panes: its acceptance progress is in the
            // turn bar of the pane that waits for you, and 待办详情 in each pane's ⋯ menu.
            let ids = current
                .map(|t| self.workspace.tab_terminals(&t.id))
                .unwrap_or_default();
            let count = ids.len();
            // The standard style sets its panes as rounded cards with a gap around them
            // (flat.html); the terminal style keeps its own frames.
            let cards = crate::ui::style::current(cx) != crate::ui::style::Style::Terminal;
            let mut grid = div()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .relative()
                .when(cards, |d| d.p(px(T::S8)).bg(T::surface(T::pane())));
            if let Some(zoomed) = &self.nav.zoomed {
                grid = grid.child(self.pane_view(zoomed, false, window, cx));
            } else if let Some(tree) = current.and_then(|t| t.layout.as_ref()) {
                grid = grid.child(self.layout_view(tree, vec![], count > 1, window, cx));
            } else if self.first_run.on {
                grid = grid.child(self.first_run_view(cx));
            } else {
                grid = grid.child(self.empty_tab(cx));
            }
            main = main.child(grid);
        }
        main
    }
}
