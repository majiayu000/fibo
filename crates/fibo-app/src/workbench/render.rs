//! The work view: top bar, sidebar, tabs, the pane grid and the footer.
use super::*;

mod bars;
mod client_frame;
mod keys;
mod launch;
mod layer_keys;
mod main_area;
mod strip;
mod tabs;
pub(super) use bars::marked_button;
pub(super) use client_frame::resize_edges;
pub(super) use tabs::TextTip;
#[cfg(feature = "visual-test")]
pub(super) use tabs::hidden_tabs;

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Another machine's spaces picked: its own workbench beside this window's sidebar.
        if let Some(view) = self.hosted_view() {
            return self.hosted_frame(view, window, cx);
        }
        self.settle_hosts(window, cx);
        self.follow_space_theme(cx);
        self.settle_changes(window, cx);
        self.before_render(window, cx);
        self.settle_board_transients(cx);
        self.return_to_queue(cx);
        // The overview and 逐个清理 take the main area beside the sidebar; the top bar,
        // sidebar and tab strip stay, so leaving is a click in the same place or Esc.
        let page = if self.tasks.form.is_some() {
            None
        } else if !self.overview.cleanup.is_empty() {
            Some(self.cleanup_view(cx))
        } else if self.nav.overview {
            Some(self.overview_view(window, cx))
        } else {
            None
        };
        if self.nav.pending_focus
            && page.is_some()
            // Nothing arriving through here takes the keyboard from the card being written.
            && self.tasks.board_quick.is_none()
            && !self.search.is_open()
            && self.diagnostics.is_none()
            && self.settings.is_none()
        {
            window.focus(&self.overview.focus, cx);
            self.nav.pending_focus = false;
        }
        if self.nav.pending_focus
            && self.diagnostics.is_none()
            && self.settings.is_none()
            && self.tasks.form.is_none()
            && self.layout.renaming_tab.is_none()
            && self.layout.renaming_session.is_none()
            && self.layout.renaming_space.is_none()
            && !self.nav.overview
        {
            if let Some(pane) = self
                .nav
                .focused
                .as_ref()
                .and_then(|id| self.layout.panes.get(id))
            {
                window.focus(&pane.focus_handle(cx), cx);
            }
            self.nav.pending_focus = false;
        }
        let terminal_style = crate::ui::style::current(cx) == crate::ui::style::Style::Terminal;
        let mut root = Self::window_root(cx);
        let space_name = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == self.nav.space)
            .map(|s| s.name.as_str())
            .unwrap_or(t!("工作空间"));
        // The main column's own title row, as T3 Code has it: where you are on the left,
        // what needs you and the overview on the right. The window's top edge belongs to
        // the sidebar; collapsed, its toggle stays beside the traffic lights right here.
        let collapsed = self.sidebar_folded();
        // Standard style puts the tabs in this row, so the current tab is not named a
        // second time; only the overview and a task's page, which no tab stands for.
        let here = if self.nav.overview || !self.overview.cleanup.is_empty() {
            Some(t!("鸟瞰全部").to_owned())
        } else if let Some(task) = self
            .nav
            .task_detail
            .as_ref()
            .and_then(|id| self.workspace.tasks.iter().find(|t| &t.id == id))
        {
            Some(task.title.clone())
        } else if terminal_style {
            self.workspace
                .tabs
                .iter()
                .find(|t| Some(&t.id) == self.nav.tab.as_ref())
                .map(|t| self.tab_name(t, cx))
        } else {
            None
        };
        // Standard style names the space in the sidebar only; the row starts with the tabs
        // unless a page no tab stands for is open.
        let show_crumb = terminal_style || here.is_some();
        let crumb = div()
            .min_w_0()
            .flex_shrink_0()
            .max_w(px(T::TAB_MAX_WIDTH * 2.))
            .flex()
            .items_center()
            .gap(px(T::S8))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(T::color(T::muted()))
                    .child(space_name.to_owned()),
            )
            .when_some(here, |d, here| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .text_color(T::color(T::muted()))
                        .child("/"),
                )
                .child(div().min_w_0().truncate().child(here))
            });
        // The right end of the title row: notes, what needs you, the overview, phones.
        let mut tail: Vec<gpui::AnyElement> = Vec::new();
        if self.store.slow() {
            tail.push(label(t!("正在更新…")).into_any_element());
        }
        if matches!(
            self.store.recognition().phase,
            RecognitionPhase::Starting | RecognitionPhase::Recovering
        ) {
            tail.push(label(t!("正在识别会话…")).into_any_element());
        } else if self.store.recognition().phase == RecognitionPhase::Interrupted
            && !self.store.link_down()
        {
            // The host is reachable but cannot read some Agent logs; the status bar's
            // tooltip carries its reason. Not 「后台连接断开」: every connection works.
            tail.push(label(t!("会话识别中断")).into_any_element());
        }
        // The host runs older code than this app. One that can hand over is updated
        // in place; an older one must be restarted, which ends every terminal.
        if crate::lifecycle::stale_host() {
            tail.push(
                button(
                    "restart-host",
                    match (
                        self.restarting,
                        crate::lifecycle::stale_build().is_some_and(|b| b.handoff),
                    ) {
                        (true, _) => t!("正在更新后台…"),
                        // Updated in place: every terminal keeps running.
                        (false, true) => t!("后台有更新 · 更新"),
                        (false, false) => t!("后台有更新 · 重启"),
                    },
                )
                .text_color(T::color(T::secondary()))
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.restarting {
                        this.request_close(Closing::Host, cx);
                    }
                }))
                .into_any_element(),
            );
        }
        if !crate::lifecycle::stale_host()
            && let Some(chip) = self.launch_chip(cx)
        {
            tail.push(chip);
        }
        if self.anything_waits() {
            tail.push(self.next_waiting_chip(cx).into_any_element());
        }
        // A toggle: the same spot goes into the overview and back out of it. Standard
        // style names it, so the way to every space's work is not a bare glyph.
        tail.push({
            let on = self.nav.overview || !self.overview.cleanup.is_empty();
            let toggle = icon_button(
                "overview",
                "icons/grid.svg",
                if on {
                    t!("回到工作区")
                } else {
                    t!("鸟瞰全部")
                },
            )
            .when(!terminal_style, |d| {
                d.w_auto()
                    .px(px(T::S8))
                    .gap(px(T::S6))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary()))
                    .child(t!("鸟瞰"))
            })
            .when(on, |d| d.bg(T::color(T::selection())))
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    TextTip(
                        if on {
                            t!("回到工作区")
                        } else {
                            t!("鸟瞰全部")
                        }
                        .into(),
                    )
                })
                .into()
            })
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_overview(on, cx)));
            self.record_toggle(toggle, cx).into_any_element()
        });
        // Phones are reached from the sidebar's foot (连接手机); once one is paired its
        // button stays up here too, showing who is online.
        if !collapsed {
            tail.extend(
                self.phones_button(cx)
                    .filter(|_| !self.store.recognition().remote.devices.is_empty())
                    .map(|b| b.into_any_element()),
            );
        } else {
            tail.extend(self.phones_button(cx).map(|b| b.into_any_element()));
        }
        let sidebar = self.cached_sidebar(window, cx);
        let current = self
            .workspace
            .tabs
            .iter()
            .find(|t| Some(&t.id) == self.nav.tab.as_ref());
        let tabs = self.cached_strip(terminal_style);
        let (row_tabs, bottom_tabs) = if terminal_style {
            (None, Some(tabs))
        } else {
            (Some(tabs), None)
        };
        let header = div()
            .h(px(T::TOP))
            .flex_shrink_0()
            .pl(px(if collapsed {
                T::TRAFFIC_LIGHT_INSET
            } else {
                T::S16
            }))
            .pr(px(T::S14))
            .flex()
            .items_center()
            .gap(px(T::S10))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .when(collapsed, |d| d.child(self.sidebar_toggle(cx)))
            .when(show_crumb, |d| d.child(crumb))
            .when_some(row_tabs, |d, tabs| {
                d.when(show_crumb, |d| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .w(px(T::BORDER_WIDTH))
                            .h(px(T::S16))
                            .bg(T::color(T::border())),
                    )
                })
                .children(tabs)
            })
            .when(terminal_style, |d| d.child(self.title_drag_area(cx)))
            .children(tail)
            .when(fibo_platform::desktop::DRAWS_WINDOW_BUTTONS, |d| {
                d.pr_0().child(bars::window_buttons(window))
            })
            .when(client_frame::draws_frame(window), |d| {
                d.pr_0().child(client_frame::buttons(window))
            });
        let mut main = self.main_area(terminal_style, header, page, current, window, cx);
        main = main.children(bottom_tabs.flatten());
        if let Some(peek) = &self.peek {
            main = main.relative().child(self.peek_view(peek, cx));
        }
        // Beside 会话优先's column the status bar belongs to the main column, so the
        // column runs to the window's foot (flat.html); otherwise it spans the window.
        // Drawn embedded, the window's own sidebar takes its share of the width.
        let width = f32::from(window.viewport_size().width) - self.hosts.beside.unwrap_or(0.);
        let beside = sidebar.is_some()
            && self.sidebar.preset().group == fibo_core::sidebar::Grouping::Recent;
        if beside {
            main = main.child(self.statusbar_view(width - T::SIDEBAR, cx));
        }
        root = root
            .relative()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .children(sidebar)
                    .child(main),
            )
            .when(!beside, |d| d.child(self.statusbar_view(width, cx)))
            .children(self.turning_view(cx));
        let root = self.window_layers(root, window, cx);
        // Embedded, this is another machine's main area beside the window's sidebar: the
        // window's own frame (`hosted_frame`) holds the edges.
        if self.hosts.embedded {
            root
        } else {
            client_frame::resize_edges(root, window)
        }
    }
}
