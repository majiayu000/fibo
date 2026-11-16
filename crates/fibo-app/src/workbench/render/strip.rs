//! The current workbench's tab strip, in the title row or below the panes.
use super::*;

impl Workbench {
    pub(super) fn tab_strip(
        &self,
        terminal_style: bool,
        page_open: bool,
        space_name: &str,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
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
        for (n, tab) in self.strip_tabs(cx).into_iter().enumerate() {
            let id = tab.id.clone();
            // Over the overview no tab is shown; clicking one goes straight back to it.
            let active = !page_open && Some(&id) == self.nav.tab.as_ref();
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
                    (
                        *attention,
                        std::cmp::Reverse(crate::workbench::layout::urgency(*g)),
                    )
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
            // Carried, the tab keeps its place as the gap and its contents step aside.
            let carried = self.tab_carried(&id, cx);
            let tip = self.tab_tip(&title, &ids);
            let dragged = self.dragged_tab(&id, &title, agent, terminal_style);
            let group_name = gpui::SharedString::from(format!("tab-group-{id}"));
            // Shown on the current tab, and on the others while hovered, like a browser.
            let close = button(gpui::SharedString::from(format!("tab-close-{id}")), "×")
                .aria_label(t!("关闭标签 {title}", title))
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
                .min_by_key(|(g, _)| crate::workbench::layout::urgency(*g))
                .map(|(g, _)| g)
                .filter(|_| !active && !terminal_style);
            let tail = if terminal_style {
                close.into_any_element()
            } else {
                let mark = self.tab_mark(after, &id, cx);
                tabs::tab_tail(close, mark, group_name.clone()).into_any_element()
            };
            strip = strip.child(
                div()
                    .id(gpui::SharedString::from(format!("tab-{id}")))
                    .role(gpui::accesskit::Role::Button)
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(T::S12))
                    .rounded(px(T::CONTROL_RADIUS))
                    .cursor_pointer()
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
                    .when(terminal_style, |d| {
                        d.rounded_none()
                            .when(!carried, |d| d.hover(|s| s.bg(T::color(T::button_hover()))))
                    })
                    .when(!terminal_style, |d| {
                        d.pl(px(T::S10 - T::BORDER_WIDTH))
                            .pr(px(T::S4 - T::BORDER_WIDTH))
                            .border(px(T::BORDER_WIDTH))
                            .border_color(gpui::transparent_black())
                            .font_weight(gpui::FontWeight::MEDIUM)
                            // Not under a carried tab's gap, which stays bare.
                            .when(!carried, |d| {
                                d.hover(move |s| {
                                    s.bg(if active {
                                        T::surface(T::pane())
                                    } else {
                                        T::color(T::todo())
                                    })
                                })
                            })
                    })
                    .text_size(px(T::MONO_SIZE))
                    .text_color(T::color(if active { T::primary() } else { T::secondary() }))
                    .when_some(agent.filter(|_| !terminal_style && marks), |d, a| {
                        d.child(crate::workbench::layout::veil(
                            carried,
                            crate::workbench::board::agent_mark(
                                a,
                                T::color(if active { T::secondary() } else { T::muted() }),
                            ),
                        ))
                    })
                    // Terminal style: `2:标题*` with the state as a symbol, like tmux.
                    .when(terminal_style, |d| {
                        d.when_some(status, |d, g| {
                            d.child(crate::workbench::layout::veil(carried, glyph(g)))
                        })
                        .when(active, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                    })
                    .child(
                        div()
                            .flex_grow_1()
                            .min_w_0()
                            .truncate()
                            .when(carried, |d| d.invisible())
                            .child(if terminal_style {
                                format!("{}:{title}{}", n + 1, if active { "*" } else { "" })
                            } else {
                                title
                            }),
                    )
                    .child(crate::workbench::layout::veil(carried, tail))
                    // The current tab: white with a hairline, the others bare.
                    .when(active && !terminal_style, |d| {
                        d.bg(T::surface(T::pane()))
                            .border_color(T::color(T::border()))
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
                    .map(|d| self.tab_hover(d, &id, cx))
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
            .map(|d| self.strip_lift(d, terminal_style, cx))
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
        let new_tab = button("new", not_ui!("＋"))
            .aria_label(t!("新建终端标签"))
            .flex_shrink_0()
            .border_0()
            .text_color(T::color(T::link()))
            .on_click(cx.listener(|this, _, _, cx| this.launch(false, None, cx)))
            .map(|d| self.new_tab_landing(d, cx));
        // Terminal style puts the tabs in a status line under the panes, tmux-like:
        // the space, the tabs, then what needs you on the right.
        if terminal_style {
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
        }
    }
}
