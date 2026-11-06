//! The work view: top bar, sidebar, tabs, the pane grid and the footer.
use super::*;

mod bars;
mod keys;
mod tabs;
pub(super) use tabs::TextTip;
#[cfg(feature = "visual-test")]
pub(super) use tabs::hidden_tabs;

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_space_theme(cx);
        self.settle_changes(window, cx);
        // Whenever the current tab changes (a click, the sidebar, a shortcut, a new tab),
        // scroll the strip so that tab is in view: the strip always shows where you are.
        if self.layout.revealed_tab != self.nav.tab {
            if let Some(ix) = self.nav.tab.as_ref().and_then(|id| {
                self.workspace
                    .tabs
                    .iter()
                    .filter(|t| t.space == self.nav.space)
                    .position(|t| &t.id == id)
            }) {
                self.layout.tab_scroll.scroll_to_item(ix);
            }
            self.layout.revealed_tab = self.nav.tab.clone();
        }
        self.before_render(window, cx);
        self.settle_board_transients(cx);
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
        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(T::base())
            .text_color(T::color(T::primary()))
            .text_size(px(T::BODY_SIZE))
            .line_height(gpui::relative(T::LINE_HEIGHT))
            .font_family(T::UI_FONT)
            .when(terminal_style, |d| d.font(T::mono(cx)));
        let space_name = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == self.nav.space)
            .map(|s| s.name.as_str())
            .unwrap_or("工作空间");
        // The main column's own title row, as T3 Code has it: where you are on the left,
        // what needs you and the overview on the right. The window's top edge belongs to
        // the sidebar; collapsed, its toggle stays beside the traffic lights right here.
        let collapsed = self.sidebar.collapsed;
        // Standard style puts the tabs in this row, so the current tab is not named a
        // second time; only the overview and a task's page, which no tab stands for.
        let here = if self.nav.overview || !self.overview.cleanup.is_empty() {
            Some("鸟瞰全部".to_owned())
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
            tail.push(label("正在更新…").into_any_element());
        }
        if matches!(
            self.store.recognition().phase,
            RecognitionPhase::Starting | RecognitionPhase::Recovering
        ) {
            tail.push(label("正在识别会话…").into_any_element());
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
                        (true, _) => "正在更新后台…",
                        // Updated in place: every terminal keeps running.
                        (false, true) => "后台有更新 · 更新",
                        (false, false) => "后台有更新 · 重启",
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
                    "回到工作区"
                } else {
                    "鸟瞰全部"
                },
            )
            .when(!terminal_style, |d| {
                d.w_auto()
                    .px(px(T::S8))
                    .gap(px(T::S6))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary()))
                    .child("鸟瞰")
            })
            .when(on, |d| d.bg(T::color(T::selection())))
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    TextTip(
                        if on {
                            "回到工作区"
                        } else {
                            "鸟瞰全部"
                        }
                        .into(),
                    )
                })
                .into()
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if on {
                    this.leave_overview(cx);
                } else {
                    this.show_overview(cx);
                }
            }));
            self.record_toggle(toggle, cx).into_any_element()
        });
        // Phones are reached from the sidebar's foot (连接手机); once one is paired its
        // button stays up here too, showing who is online.
        if !self.sidebar.collapsed {
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
        // Long conversation titles stay on one row: each tab is capped and elided, and
        // the strip scrolls sideways while 「＋」 and 「收起标签」 stay put.
        let mut strip = div()
            .id("tab-strip")
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .when(!terminal_style, |d| d.gap(px(T::S2)))
            .overflow_x_scroll()
            .track_scroll(&self.layout.tab_scroll);
        // What each tab needs from you, in strip order, so a hidden one can be called out.
        let mut wants = Vec::new();
        // Every row and tab shows its CLI's mark, in its own color (v3 mock).
        let marks = true;
        for (n, tab) in self
            .workspace
            .tabs
            .iter()
            .filter(|t| t.space == self.nav.space)
            .enumerate()
        {
            let id = tab.id.clone();
            // Over the overview no tab is shown; clicking one goes straight back to it.
            let active = page.is_none() && Some(&id) == self.nav.tab.as_ref();
            let ids = self.workspace.tab_terminals(&id);
            let needs: Vec<_> = self
                .current_on(&ids)
                .into_iter()
                .filter(|s| {
                    s.outcome.needs_review(now())
                        && matches!(
                            status_of(&self.workspace, s),
                            Status::Waiting | Status::Error
                        )
                })
                .collect();
            let group = needs.first().map(|s| status_of(&self.workspace, s));
            wants.push(group);
            // The tab's dot: the most urgent status among its panes; a tab of plain
            // shells keeps a faint neutral dot so the names stay aligned.
            let status = ids
                .iter()
                .filter_map(|id| self.pane_status(id))
                .max_by_key(|(g, attention)| {
                    (*attention, std::cmp::Reverse(super::layout::urgency(*g)))
                })
                .map(|(g, _)| g);
            // A recognized CLI has a mark before its first conversation, too.
            let agent = ids.iter().find_map(|id| self.pane_agent(id));
            if let Some((_, field, _)) =
                self.layout.renaming_tab.as_ref().filter(|(t, ..)| t == &id)
            {
                // Edited in place: the tab keeps its height, padding and icon, and only
                // its name turns editable, outlined so the edit is plain to see. A long
                // name gets wider room while typed; the strip scrolls to make space.
                strip = strip.child(
                    div()
                        .flex_shrink_0()
                        .min_w(px(T::TAB_RENAME_MIN))
                        .max_w(px(T::TAB_RENAME_MAX))
                        .h(px(T::TABS))
                        .px(px(T::S12 - T::BORDER_WIDTH))
                        .flex()
                        .items_center()
                        .gap(px(T::S6))
                        .bg(T::color(T::raised()))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::focus()))
                        .text_size(px(T::MONO_SIZE))
                        .text_color(T::color(T::primary()))
                        .child(div().flex_1().min_w_0().h_full().child(field.clone())),
                );
                continue;
            }
            let title = self.tab_name(tab, cx);
            let tip = self.tab_tip(&title, &ids);
            let dragged = super::tabs::DraggedTab {
                id: id.clone(),
                title: title.clone(),
            };
            let group_name = gpui::SharedString::from(format!("tab-group-{id}"));
            // Shown on the current tab, and on the others while hovered, like a browser.
            let close = button(gpui::SharedString::from(format!("tab-close-{id}")), "×")
                .aria_label(format!("关闭标签 {title}"))
                .flex_shrink_0()
                .h(px(T::ICON_HEIGHT))
                .px(px(T::S4))
                .text_color(T::color(T::secondary()))
                .when(!active, |d| {
                    d.opacity(0.)
                        .group_hover(group_name.clone(), |s| s.opacity(1.))
                })
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click({
                    let id = id.clone();
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_tab(&id, cx);
                    })
                });
            // Standard: after the name, the state of a pane that needs you or is working,
            // as the sidebar's rows end with it; the current tab shows × there instead.
            let after = ids
                .iter()
                .filter_map(|id| self.pane_status(id))
                .filter(|(g, attention)| *attention || g.working())
                .min_by_key(|(g, _)| super::layout::urgency(*g))
                .map(|(g, _)| g)
                .filter(|_| !active && !terminal_style);
            strip = strip.child(
                button(gpui::SharedString::from(format!("tab-{id}")), "")
                    .group(group_name)
                    .aria_label(title.clone())
                    .relative()
                    .flex_shrink_0()
                    .max_w(px(if terminal_style {
                        T::TAB_MAX_WIDTH
                    } else {
                        T::TAB_SHORT
                    }))
                    .h(px(if terminal_style { T::FOOTER } else { T::ROW }))
                    .gap(px(T::S6))
                    .border_0()
                    .when(terminal_style, |d| d.rounded_none())
                    .when(!terminal_style, |d| {
                        d.pl(px(T::S10 - T::BORDER_WIDTH))
                            .pr(px(T::S4 - T::BORDER_WIDTH))
                            .border(px(T::BORDER_WIDTH))
                            .border_color(gpui::transparent_black())
                    })
                    .text_size(px(T::MONO_SIZE))
                    .text_color(T::color(if active { T::primary() } else { T::secondary() }))
                    .when_some(agent.filter(|_| !terminal_style && marks), |d, a| {
                        d.child(super::board::agent_mark(
                            a,
                            T::color(if active { T::secondary() } else { T::muted() }),
                        ))
                    })
                    // Terminal style: `2:标题*` with the state as a symbol, like tmux.
                    .when(terminal_style, |d| {
                        d.when_some(status, |d, g| d.child(glyph(g)))
                            .when(active, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                    })
                    .child(
                        div()
                            .flex_grow_1()
                            .min_w_0()
                            .truncate()
                            .child(if terminal_style {
                                format!("{}:{title}{}", n + 1, if active { "*" } else { "" })
                            } else {
                                title
                            }),
                    )
                    .when_some(after, |d, g| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(T::CHIP_SIZE))
                                .pr(px(T::S4))
                                .child(crate::ui::widgets::status_mark(g)),
                        )
                    })
                    .when(active || terminal_style, |d| d.child(close))
                    // The current tab: white with a hairline, the others bare.
                    .when(active && !terminal_style, |d| {
                        d.bg(T::surface(T::pane()))
                            .border_color(T::color(T::border()))
                            .font_weight(gpui::FontWeight::MEDIUM)
                    })
                    // The hover card would cover the right-click menu; dropping the
                    // builder while a menu is open also dismisses a visible card.
                    .when(self.layout.tab_menu.is_none(), |d| {
                        d.tooltip(move |_, cx| cx.new(|_| tip.clone()).into())
                    })
                    .on_mouse_down(gpui::MouseButton::Middle, {
                        let id = id.clone();
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.close_tab(&id, cx);
                        })
                    })
                    // Tabs move along the strip; panes dropped on one join it.
                    .map(|d| self.tab_landing(d, dragged, cx))
                    .on_mouse_down(gpui::MouseButton::Right, {
                        let id = id.clone();
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            this.layout.tab_menu = Some((id.clone(), event.position));
                            cx.stop_propagation();
                            cx.notify();
                        })
                    })
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if event.click_count() == 2 {
                                this.rename_tab(&id, window, cx);
                                return;
                            }
                            this.show_tab(&id, window, cx);
                        }),
                    ),
            );
        }
        let (left, right) = self.tab_overflow(&wants, cx);
        // The left marker floats over the strip so its coming and going never shifts
        // the tabs a click on either marker has just brought into view.
        let fade = right.is_some() && !terminal_style;
        let strip = div()
            .relative()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .child(strip.flex_1())
            // Tabs past the right edge fade out instead of being cut mid-character.
            .when(fade, |d| {
                d.child(
                    div()
                        .absolute()
                        .right_0()
                        .top_0()
                        .h_full()
                        .w(px(T::S20 * 2.))
                        .bg(gpui::linear_gradient(
                            90.,
                            gpui::linear_color_stop(T::surface(T::pane()).opacity(0.), 0.),
                            gpui::linear_color_stop(T::surface(T::pane()), 1.),
                        )),
                )
            })
            .when_some(left, |d, hint| {
                d.child(
                    self.tab_edge(hint, false, terminal_style, cx)
                        .absolute()
                        .left_0()
                        .top_0(),
                )
            });
        let right = right.map(|hint| self.tab_edge(hint, true, terminal_style, cx));
        let new_tab = button("new", "＋")
            .aria_label("新建终端标签")
            .flex_shrink_0()
            .border_0()
            .text_color(T::color(T::link()))
            .on_click(cx.listener(|this, _, _, cx| this.launch(false, None, cx)))
            .map(|d| self.new_tab_landing(d, cx));
        // Terminal style puts the tabs in a status line under the panes, tmux-like:
        // the space, the tabs, then what needs you on the right.
        let tabs = if terminal_style {
            div()
                .h(px(T::FOOTER))
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(T::S8))
                .px(px(T::S8))
                .border_t(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .bg(T::surface(T::panel()))
                .text_size(px(T::MONO_SIZE))
                .child(label(space_name.to_owned()).flex_shrink_0())
                .child(strip.flex_1())
                .children(right)
                .child(new_tab)
                .child(self.status_glyphs(cx))
        } else {
            // Standard: the tabs share the title row, T3 Code style. The strip keeps its
            // own width so the space after 「＋」 still moves the window.
            div()
                .h_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(T::S2))
                .child(strip)
                .children(right)
                .child(new_tab)
        };
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
                .child(tabs)
                .child(self.title_drag_area(cx).min_w(px(T::S20 * 2.)))
            })
            .when(terminal_style, |d| d.child(self.title_drag_area(cx)))
            .children(tail);
        let mut main = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            // One white surface under the title row and the panes; the sidebar's gray
            // beside it is the only other layer.
            .when(!terminal_style, |d| d.bg(T::surface(T::pane())))
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
            let mut details = vec![self.store.recognition().detail.clone()];
            details.extend(status);
            main = main.child(self.notice_strip("后台连接断开，正在重新连接…", details, true, cx));
        } else if let Some(status) = status.filter(|_| {
            // A failed refresh while the link is down for a moment is the same blip.
            self.store.recognition().phase != RecognitionPhase::Interrupted
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
            } else {
                grid = grid.child(self.empty_tab(cx));
            }
            main = main.child(grid);
        }
        main = main.children(bottom_tabs);
        if let Some(peek) = &self.peek {
            main = main.relative().child(self.peek_view(peek, cx));
        }
        // Beside 会话优先's column the status bar belongs to the main column, so the
        // column runs to the window's foot (flat.html); otherwise it spans the window.
        let width = f32::from(window.viewport_size().width);
        let beside = sidebar.is_some()
            && self.sidebar.preset().group == mooring_core::sidebar::Grouping::Recent;
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
        // 标记完成 runs leave their panes for the sidebar: over everything but the menus.
        if let Some(overlay) = self.completing_overlay(window) {
            root = root.child(overlay);
        }
        if let Some(menu) = &self.layout.menu {
            root = root.child(self.menu_view(menu, window, cx));
        }
        if let Some(position) = self.sidebar.view_menu {
            root = root.child(self.view_menu_view(position, window, cx));
        }
        if self.phones.open {
            root = root.child(self.phones_panel(window, cx));
        }
        root = root.children(self.overlays(cx));
        if let Some(position) = self.layout.icon_menu {
            root = root.child(self.icon_menu_view(position, window, cx));
        }
        if let Some((id, position)) = &self.layout.tab_menu {
            root = root.child(self.tab_menu_view(id, *position, window, cx));
        }
        if let Some((id, position)) = &self.layout.session_menu {
            root = root.child(self.session_menu_view(id, *position, window, cx));
        }
        root = root.children(self.sidebar_menus(window, cx));
        if let Some(closing) = &self.layout.confirm_close {
            root = root.child(self.confirm_close_view(closing, cx));
        }
        if self.search.is_open() {
            root = root.child(self.search_palette(cx));
        }
        if let Some(view) = &self.diagnostics {
            root = root.child(self.diagnostic_overlay(view.clone(), cx));
        }
        if let Some(view) = &self.settings {
            root = root.child(self.settings_overlay(view.clone(), cx));
        }
        root.on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            if let Some(action) = crate::keymap::action(&event.keystroke, cx)
                && this.shortcut(action, window, cx)
            {
                cx.stop_propagation();
            }
        }))
        .on_mouse_move(cx.listener(Self::resize_drag))
        .on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(|this, _, _, cx| this.finish_drag(cx)),
        )
        .on_mouse_up_out(
            gpui::MouseButton::Left,
            cx.listener(|this, _, _, cx| this.finish_drag(cx)),
        )
    }
}
