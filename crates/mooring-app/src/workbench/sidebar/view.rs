//! The column itself: its header, search, spaces and sessions, and its foot.
use super::*;

impl Workbench {
    pub(in crate::workbench) fn sidebar_view(
        &self,
        space_name: &str,
        cx: &Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        // Collapsed means gone: the toggle moves to the main title row.
        if self.sidebar.collapsed {
            return None;
        }
        let dense = self.sidebar.dense();
        // 会话优先, the default: the spaces as lettered tiles on top, then every space's
        // sessions in one list, each row naming its space.
        let recent = self.sidebar.preset().group == mooring_core::sidebar::Grouping::Recent;
        let all_spaces = self.sidebar.preset().all_spaces();
        let mut sidebar = div()
            .id("sidebar-content")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.sidebar.scroll.handle)
            .when(!recent, |d| {
                d.px(px(T::S8))
                    .py(px(if dense { T::S8 } else { T::S12 }))
                    .gap(px(T::S2))
            })
            .when(recent, |d| d.pb(px(T::S12)))
            .flex()
            .flex_col()
            .font_features(super::diagnostics::tabular());
        if recent {
            sidebar = sidebar.children(self.column_top(cx));
        } else if dense {
            // The chips' own margins are the flat column's, which has no padding of its own.
            sidebar = sidebar
                .when(all_spaces, |d| {
                    d.child(div().mx(px(-T::S8)).children(self.space_tiles(cx)))
                })
                .child(self.space_line(all_spaces, cx));
        } else {
            sidebar = sidebar.child(
                div()
                    .flex()
                    .px(px(T::S8))
                    .mb(px(T::S4))
                    .child(label("空间").flex_1())
                    .child(label(self.workspace.spaces.len().to_string())),
            );
        }
        for space in self.workspace.spaces.iter().filter(|_| !dense) {
            let id = space.id.clone();
            let active = self.space_active(&id);
            let SpaceCounts {
                count,
                waiting,
                errors,
                group,
            } = self.space_counts(&id);
            if let Some((_, field, _)) = self
                .layout
                .renaming_space
                .as_ref()
                .filter(|(s, ..)| s.id == id)
            {
                // Named in place, like a tab: the row keeps its height and dot, and only
                // the name turns editable, outlined so the edit is plain to see.
                sidebar = sidebar.child(
                    div()
                        .h(px(T::ROW))
                        .px(px(T::S8 - T::BORDER_WIDTH))
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        .rounded(px(T::CONTROL_RADIUS))
                        .bg(T::color(T::raised()))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::focus()))
                        .text_size(px(T::SMALL))
                        .child(dot(group))
                        .child(div().flex_1().min_w_0().h_full().child(field.clone())),
                );
                continue;
            }
            let missing = space_dir_missing(space);
            sidebar = sidebar.child(
                button(gpui::SharedString::from(format!("space-{id}")), "")
                    .aria_label(space.name.clone())
                    .gap(px(T::S8))
                    .child(dot(group))
                    .child(div().min_w_0().truncate().child(space.name.clone()))
                    // The branch the space's directory is on.
                    .when_some(self.git_branch(&space.cwd, cx), |d, branch| {
                        d.child(
                            label(branch)
                                .min_w_0()
                                .max_w(px(T::SIDEBAR * 0.4))
                                .truncate()
                                .text_size(px(T::CHIP_SIZE))
                                .text_color(T::color(T::muted())),
                        )
                    })
                    .when_some(
                        self.git_divergence(&space.cwd, cx)
                            .and_then(super::divergence::divergence_label),
                        |d, counts| {
                            d.child(
                                label(counts)
                                    .flex_shrink_0()
                                    .text_size(px(T::CHIP_SIZE))
                                    .text_color(T::color(T::muted()))
                                    .font_features(T::tabular()),
                            )
                        },
                    )
                    // A deleted worktree leaves the space pointing nowhere; new terminals
                    // fall back to home, and a click here picks the folder again.
                    .when(missing, |d| {
                        let id = id.clone();
                        d.child(
                            button(
                                gpui::SharedString::from(format!("space-missing-{id}")),
                                "目录不在了",
                            )
                            .aria_label(format!("重新选择 {} 的目录", space.name))
                            .flex_shrink_0()
                            .h(px(T::ICON_HEIGHT))
                            .px(px(T::S4))
                            .border_0()
                            .rounded(px(T::CHIP_RADIUS))
                            .bg(T::color(T::chip()))
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::secondary()))
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.choose_space_dir(Some(id.clone()), window, cx);
                                },
                            )),
                        )
                    })
                    .justify_start()
                    .px(px(T::S8))
                    .border_0()
                    .when(active, |s| s.bg(T::color(T::raised())))
                    .child(div().flex_1())
                    .when(waiting > 0, |d| d.child(badge(Status::Waiting, waiting)))
                    .when(errors > 0, |d| d.child(badge(Status::Error, errors)))
                    .child(
                        label(count.to_string())
                            .flex_shrink_0()
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted())),
                    )
                    .on_mouse_down(gpui::MouseButton::Right, {
                        let id = id.clone();
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            this.layout.space_menu = Some((id.clone(), event.position));
                            this.layout.space_switch = false;
                            this.layout.space_removing = false;
                            cx.stop_propagation();
                            cx.notify();
                        })
                    })
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if event.click_count() == 2 {
                                this.rename_space(&id, window, cx);
                                return;
                            }
                            this.pick_space(&id, cx);
                        }),
                    ),
            );
        }
        if !dense {
            sidebar = sidebar.child(
                button("add-space", "＋ 空间")
                    .border_0()
                    .justify_start()
                    .text_color(T::color(T::muted()))
                    .on_click(cx.listener(|this, _, window, cx| this.new_space(window, cx))),
            );
        }
        // Sorted by what each row shows. Within a state, newest started first: the
        // host keeps sessions in creation order, while `updated` ticks with every
        // output line and would move rows under the pointer.
        // Only sessions open in a tab; closed ones live in the overview, ready to resume.
        let local_hidden = self.local_hidden();
        let in_space: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                !local_hidden
                    && self.session_hidden(s).is_none()
                    && match (all_spaces, self.space_filter()) {
                        (true, None) => true,
                        (true, Some(only)) => self.workspace.session_space(s) == Some(only),
                        (false, _) => {
                            self.workspace.session_space(s) == Some(self.nav.space.as_str())
                        }
                    }
            })
            .collect();
        let closed = in_space
            .iter()
            .filter(|(_, s)| self.session_closed(s) && closed_recently(&self.workspace, s))
            .count();
        // A WASM preset is handed the newest closed sessions too, to place as it likes.
        let mut ended: Vec<_> = in_space
            .iter()
            .filter(|(_, s)| self.session_closed(s))
            .map(|(_, s)| {
                let (g, attention) = session_status(&self.workspace, &self.attention.unseen, s);
                (*s, g, attention)
            })
            .collect();
        ended.sort_by_key(|(s, ..)| std::cmp::Reverse(self.session_updated(s)));
        let ended = super::sidebar_wasm::Closed {
            total: ended.len(),
            rows: ended
                .into_iter()
                .take(super::sidebar_wasm::CLOSED_SHOWN)
                .collect(),
        };
        let mut sessions: Vec<_> = in_space
            .into_iter()
            .filter(|(_, s)| self.session_open(s))
            .map(|(i, s)| {
                let (g, attention) = session_status(&self.workspace, &self.attention.unseen, s);
                (s, g, attention, i)
            })
            .collect();
        // Newest first inside a tier, by the time the row shows. Live work keys on when
        // its turn began, which holds still while it prints, so two running rows never
        // trade places under the pointer.
        sessions.sort_by_key(|(s, g, attention, i)| {
            let at = match g {
                Status::Running => self.turn_started(s),
                _ => None,
            }
            .unwrap_or_else(|| self.session_updated(s));
            (
                sidebar_rank(*g, *attention),
                std::cmp::Reverse(at),
                std::cmp::Reverse(*i),
            )
        });
        // A peek is what is in front: its row is the selected one until it closes.
        let current = match &self.peek {
            Some(peek) => Some(peek.session.clone()),
            None => self
                .nav
                .focused
                .as_ref()
                .and_then(|id| self.current_agent(id))
                .map(|s| s.id.clone()),
        };
        // Agents running in a tab here that no evidence has tied to a conversation yet:
        // listed rather than missing, so a CLI Mooring cannot read is never silent.
        // A program that reports its own status (OSC 7501) is listed the same way, its dot
        // saying what it reported.
        let recognition = self.store.recognition();
        let mut unread: Vec<_> = recognition
            .terminal_notes
            .keys()
            .chain(recognition.terminal_reports.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|terminal| {
                !local_hidden
                    && self.current_agent(terminal).is_none()
                    && self.workspace.terminal_tab(terminal).is_some_and(|tab| {
                        match (all_spaces, self.space_filter()) {
                            (true, None) => true,
                            (true, Some(only)) => tab.space == only,
                            (false, _) => tab.space == self.nav.space,
                        }
                    })
            })
            .map(|terminal| {
                let report = recognition
                    .terminal_reports
                    .get(terminal)
                    .map(crate::model::session_state::report_view);
                let group = report.as_ref().map_or(Status::Stale, |(g, _)| *g);
                let n = recognition
                    .terminal_notes
                    .get(terminal)
                    .cloned()
                    .or_else(|| report.map(|(_, n)| n))
                    .expect("listed from notes or reports");
                super::sidebar_list::Unlinked {
                    terminal: terminal.clone(),
                    note: n.note,
                    why: n.why,
                    group,
                }
            })
            .collect();
        unread.sort_by(|a, b| a.terminal.cmp(&b.terminal));
        if !local_hidden {
            sidebar = sidebar.child(
                div()
                    .when(recent, |d| d.px(px(T::S8)))
                    .child(self.todos_view(space_name, dense, cx)),
            );
        }
        // Sessions only: the terminals with none count under their own heading.
        let listed_count = sessions.len();
        sidebar = sidebar.child(self.sessions_header(listed_count, cx));
        // 会话优先 lists closed sessions under 今天 and 更早, so the column is not empty.
        if !local_hidden && sessions.is_empty() && unread.is_empty() && !(recent && ended.total > 0)
        {
            sidebar = sidebar.child(
                label("没有打开的会话。从右边启动一个 Agent，或在终端里运行 claude、codex，它们会出现在这里")
                    .mt(px(T::S14))
                    .px(px(T::S8)),
            );
        }
        let listed = sessions.into_iter().map(|(s, g, a, _)| (s, g, a)).collect();
        let (rows, more, owns_closed) = self.session_list(listed, &ended, current.as_deref(), cx);
        // After the rows are made: it takes the marks they left.
        sidebar = sidebar
            .child(self.list_watch(current.clone()))
            .children(self.moving_rows(rows));
        if more > 0 {
            sidebar = sidebar.child(
                button(
                    "sidebar-more-sessions",
                    format!("另有 {more} 个 · 鸟瞰全部"),
                )
                .border_0()
                .justify_start()
                .when(recent, |d| d.mx(px(T::S8)))
                .text_size(px(T::META))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(|this, _, _, cx| this.show_overview(cx))),
            );
        }
        if closed > 0 && !owns_closed {
            sidebar = sidebar.child(
                button(
                    "sidebar-closed-sessions",
                    if dense {
                        format!(
                            "已关闭 {closed} · {} 找回",
                            crate::keymap::keycap(cx, crate::keymap::Action::Search)
                        )
                    } else {
                        format!("已关闭 {closed} 个 · 点开接着聊")
                    },
                )
                .border_0()
                .justify_start()
                .when(dense, |d| {
                    d.mt(px(T::S8)).px(px(T::S8)).font_family(T::UI_FONT)
                })
                .when(recent, |d| {
                    use crate::ui::theme::Flat as F;
                    d.mx(px(F::ROW_EDGE)).px(px(F::ROW_PAD))
                })
                .text_size(px(T::META))
                .text_color(T::color(T::muted()))
                .on_click(cx.listener(|this, _, _, cx| this.show_closed(cx))),
            );
        }
        // Last of this machine's list, so 另有 and 已关闭 stay with the sessions they count.
        sidebar = sidebar.children(self.unlinked_rows(unread, recent, cx));
        sidebar = sidebar.children(self.after_local_list(recent, cx));
        if recent {
            return Some(self.flat_column(sidebar, listed_count, cx));
        }
        Some(
            div()
                .id("sidebar")
                .w(px(T::SIDEBAR))
                .h_full()
                .min_h_0()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(T::surface(T::sidebar()))
                .border_r(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                // The window's top edge: the traffic lights and the name, then search
                // (⌘P) and the button that folds the column away.
                .child(
                    div()
                        .h(px(T::TOP))
                        .flex_shrink_0()
                        .pl(px(T::TRAFFIC_LIGHT_INSET))
                        .pr(px(T::S8))
                        .flex()
                        .items_center()
                        .gap(px(T::S4))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(T::CHIP_SIZE))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(T::color(T::secondary()))
                                .child("MOORING"),
                        )
                        .child(self.title_drag_area(cx))
                        .child(self.sidebar_toggle(cx)),
                )
                .child(self.sidebar_search(cx))
                .child(sidebar)
                .child(self.sidebar_foot(cx)),
        )
    }
    /// Under the column's top edge: a field-like search that opens ⌘P.
    fn sidebar_search(&self, cx: &Context<Self>) -> gpui::Div {
        div().flex_shrink_0().px(px(T::S12)).pb(px(T::S4)).child(
            div()
                .id("global-search")
                .role(gpui::accesskit::Role::Button)
                .aria_label("搜索会话、空间或原话")
                .h(px(T::ROW))
                .px(px(T::S10))
                .flex()
                .items_center()
                .gap(px(T::S8))
                .rounded(px(T::CONTROL_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .cursor_pointer()
                .hover(|s| s.bg(T::color(T::raised())))
                .text_color(T::color(T::muted()))
                .child(
                    gpui::svg()
                        .path("icons/search.svg")
                        .size(px(T::S12 + 1.))
                        .text_color(T::color(T::muted())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(T::SMALL))
                        .child("找会话、空间或原话"),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(T::mono(cx).family)
                        .text_size(px(T::CHIP_SIZE))
                        .child(crate::keymap::keycap(cx, crate::keymap::Action::Search)),
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx))),
        )
    }
    /// The column's foot, one line: the key to the next session that needs you on the
    /// left; the space's todos, the view, pairing a phone and settings as quiet buttons.
    fn sidebar_foot(&self, cx: &Context<Self>) -> gpui::Div {
        let todos = self
            .workspace
            .tasks
            .iter()
            .filter(|t| t.space == self.nav.space)
            .count();
        let todos_open = self.tasks.todos_open;
        let view_open = self.sidebar.view_menu.is_some();
        let mono = T::mono(cx).family;
        let icon = |id: &'static str, path: &'static str, name: &'static str| {
            icon_button(id, path, name)
                .size(px(T::ICON_HEIGHT + T::S4))
                .text_color(T::color(T::secondary()))
                .tooltip(move |_, cx| cx.new(|_| super::render::TextTip(name.into())).into())
        };
        div()
            .flex_shrink_0()
            .h(px(T::ROW + T::S12))
            .pl(px(T::S14))
            .pr(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S2))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(T::muted()))
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(T::S4))
                            .rounded(px(T::S4))
                            .border(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::control()))
                            .font_family(mono)
                            .text_color(T::color(T::secondary()))
                            .child(crate::keymap::keycap(
                                cx,
                                crate::keymap::Action::NextWaiting,
                            )),
                    )
                    .child(div().min_w_0().truncate().child("下一个要你的")),
            )
            .child(
                button("todos-toggle", format!("待办 {todos}"))
                    .aria_label(if todos_open {
                        "收起待办"
                    } else {
                        "展开待办"
                    })
                    .flex_shrink_0()
                    .h(px(T::ICON_HEIGHT + T::S4))
                    .px(px(T::S6))
                    .border_0()
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(T::color(if todos_open {
                        T::primary()
                    } else {
                        T::secondary()
                    }))
                    .when(todos_open, |d| d.bg(T::color(T::raised())))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tasks.todos_open = !this.tasks.todos_open;
                        cx.notify();
                    })),
            )
            .child(
                icon("sidebar-foot-view", "icons/sliders.svg", "选择视图")
                    .when(view_open, |d| d.bg(T::color(T::button_hover())))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.sidebar.view_menu = match this.sidebar.view_menu {
                                Some(_) => None,
                                None => Some(event.position),
                            };
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .child(
                icon("sidebar-phones", "icons/phone.svg", "连接手机或其他电脑")
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_phones(cx))),
            )
            .child(
                icon("sidebar-settings", "icons/gear.svg", "设置")
                    .on_click(cx.listener(|this, _, window, cx| this.show_settings(window, cx))),
            )
    }
}
