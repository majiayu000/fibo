//! The left column: spaces, the current space's sessions and its (folded) todos.
use super::*;

/// What needs you, in this order: waiting on you, failed, a finished turn not seen yet.
const NEEDS_YOU: [Status; 3] = [Status::Waiting, Status::Error, Status::Review];
/// Row tiers by the shown state: what needs you, then live work, then the rest, then
/// sessions opened but not yet spoken to, then what has gone quiet. Inside a tier the
/// newest comes first (see `sidebar_time`).
pub(super) fn sidebar_rank(g: Status, attention: bool) -> (u8, usize) {
    if attention {
        let at = NEEDS_YOU.iter().position(|o| *o == g);
        return (0, at.unwrap_or(NEEDS_YOU.len()));
    }
    match g {
        Status::Running => (1, 0),
        Status::Fresh => (3, 0),
        Status::Stale => (4, 0),
        _ => (2, 0),
    }
}
/// A closed session the sidebar still counts: no outcome yet and not gone quiet past
/// the stale days; older ones wait in the overview's 很久没动.
pub(super) fn closed_recently(w: &Workspace, s: &AgentSession) -> bool {
    s.outcome.needs_review(now()) && status_of(w, s) != Status::Stale
}

/// A space's row figures: its open sessions, those waiting on you and those failed, and
/// the state its dot shows.
struct SpaceCounts {
    count: usize,
    waiting: usize,
    errors: usize,
    group: Status,
}

impl Workbench {
    /// 鸟瞰 narrowed to this space's closed sessions, ready to resume.
    pub(super) fn show_closed(&mut self, cx: &mut Context<Self>) {
        self.show_overview(cx);
        self.overview.closed_only = true;
        self.overview.changed_only = false;
        self.overview.filter_space = Some(self.nav.space.clone());
    }
    pub(super) fn sidebar_view(
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
        let mut sidebar = div()
            .id("sidebar-content")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
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
            sidebar = sidebar.children(self.space_tiles(cx));
        } else if dense {
            sidebar = sidebar.child(self.space_line(cx));
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
        let in_space: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                self.session_hidden(s).is_none()
                    && match (recent, self.space_filter()) {
                        (true, None) => true,
                        (true, Some(only)) => session_space(&self.workspace, s) == Some(only),
                        (false, _) => {
                            session_space(&self.workspace, s) == Some(self.nav.space.as_str())
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
                .and_then(|id| self.workspace.current_agent(id))
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
                self.workspace.current_agent(terminal).is_none()
                    && self
                        .workspace
                        .terminal_tab(terminal)
                        .is_some_and(|tab| tab.space == self.nav.space)
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
                (terminal.clone(), (n.note, n.why, group))
            })
            .collect();
        unread.sort_by(|a, b| a.0.cmp(&b.0));
        sidebar = sidebar.child(
            div()
                .when(recent, |d| d.px(px(T::S8)))
                .child(self.todos_view(space_name, dense, cx)),
        );
        let listed_count = sessions.len() + unread.len();
        sidebar = sidebar.child(self.sessions_header(sessions.len() + unread.len(), cx));
        // 会话优先 lists closed sessions under 今天 and 更早, so the column is not empty.
        if sessions.is_empty() && unread.is_empty() && !(recent && ended.total > 0) {
            sidebar = sidebar.child(
                label("没有打开的会话。从右边启动一个 Agent，或在终端里运行 claude、codex，它们会出现在这里")
                    .mt(px(T::S14))
                    .px(px(T::S8)),
            );
        }
        let listed = sessions.into_iter().map(|(s, g, a, _)| (s, g, a)).collect();
        let (rows, more, owns_closed) = self.session_list(listed, &ended, current.as_deref(), cx);
        sidebar = sidebar.children(rows);
        for (terminal, (note, why, group)) in unread {
            let focused = self.nav.focused.as_deref() == Some(terminal.as_str());
            sidebar = sidebar.child(
                div()
                    .id(gpui::SharedString::from(format!("unread-row-{terminal}")))
                    .when(recent, |d| d.mx(px(T::S8)))
                    .h(px(T::ROW))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .rounded(px(T::CONTROL_RADIUS))
                    .cursor_pointer()
                    .when(focused, |d| d.bg(T::color(T::raised())))
                    .hover(|s| s.bg(T::color(T::raised())))
                    .child(seen_dot(group))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(T::MONO_SIZE))
                            .text_color(T::color(T::muted()))
                            .child(note),
                    )
                    .tooltip(move |_, cx| {
                        cx.new(|_| super::render::TextTip(why.clone().into()))
                            .into()
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus_terminal(&terminal, window, cx);
                    })),
            );
        }
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
                .when(recent, |d| d.mx(px(T::S8)).px(px(T::S10)))
                .text_size(px(T::META))
                .text_color(T::color(T::muted()))
                .on_click(cx.listener(|this, _, _, cx| this.show_closed(cx))),
            );
        }
        sidebar = sidebar.children(self.machine_rows(recent, cx));
        if recent {
            use crate::ui::theme::Flat as F;
            return Some(
                div()
                    .id("sidebar")
                    .w(px(F::WIDTH))
                    .h_full()
                    .min_h_0()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .bg(T::color(F::bg()))
                    .border_r(px(T::BORDER_WIDTH))
                    .border_color(T::color(F::line()))
                    .child(self.flat_head(cx))
                    .child(self.flat_search(cx))
                    .child(sidebar)
                    .child(self.flat_keys(cx))
                    .child(self.flat_foot(listed_count, cx)),
            );
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
                .bg(T::color(T::sidebar()))
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
    fn space_tiles(&self, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        use crate::ui::theme::Flat as F;
        let mut out: Vec<gpui::AnyElement> = vec![];
        // The spaces as chips that wrap: each its colors, its name and how many of its
        // sessions need you. A click narrows the list below to that space (and shows it on
        // the right); a second click lists every space again.
        let filter = self.space_filter().map(str::to_owned);
        let needs = crate::model::session_state::needs_by_space(&self.workspace);
        let mut chips = div()
            .flex()
            .flex_wrap()
            .gap(px(F::CHIP_GAP))
            .px(px(T::S12))
            .pt(px(T::S10))
            .pb(px(T::S2))
            .font_family(T::UI_FONT);
        for space in &self.workspace.spaces {
            let id = space.id.clone();
            if let Some((_, field, _)) = self
                .layout
                .renaming_space
                .as_ref()
                .filter(|(s, ..)| s.id == id)
            {
                // Named in place: the chip widens into a field until Enter.
                chips = chips.child(
                    div()
                        .w_full()
                        .h(px(T::ICON_HEIGHT + T::S4))
                        .px(px(T::S8 - T::BORDER_WIDTH))
                        .flex()
                        .items_center()
                        .gap(px(T::S6))
                        .rounded(px(T::CONTROL_RADIUS))
                        .bg(T::color(T::raised()))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::focus()))
                        .text_size(px(T::SMALL))
                        .child(self.space_dot(&space.id, T::S10))
                        .child(div().flex_1().min_w_0().h_full().child(field.clone())),
                );
                continue;
            }
            let on = filter.as_deref() == Some(id.as_str());
            let dimmed = filter.is_some() && !on;
            let missing = space_dir_missing(space);
            let need = needs.get(id.as_str()).copied().unwrap_or(0);
            let fix = id.clone();
            chips = chips.child(
                div()
                    .id(gpui::SharedString::from(format!("space-{id}")))
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(if missing {
                        format!("{}，目录不在了，点一下重新选择", space.name)
                    } else if on {
                        format!("{}，正在只看这个空间，点一下看全部", space.name)
                    } else {
                        format!("只看 {}", space.name)
                    })
                    .h(px(F::CHIP))
                    .max_w_full()
                    .pl(px(T::S6))
                    .pr(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(F::CHIP_GAP))
                    .rounded(px(F::CHIP / 2.))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(if on {
                        F::chip_on_line()
                    } else {
                        F::chip_line()
                    }))
                    .when(on, |d| d.bg(T::color(F::chip_on())))
                    .when(dimmed, |d| d.opacity(0.5))
                    .cursor_pointer()
                    .hover(|s| s.border_color(T::color(F::chip_on_line())).opacity(1.))
                    .child(self.space_dot(&space.id, F::CHIP_DOT))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(F::CHIP_TEXT))
                            .line_height(px(F::LINE_SMALL))
                            .text_color(if missing {
                                Tone::Danger.ink()
                            } else if on {
                                T::color(F::bright())
                            } else {
                                T::color(F::quiet())
                            })
                            .child(space.name.clone()),
                    )
                    .when(need > 0, |d| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .min_w(px(F::BADGE))
                                .h(px(F::BADGE))
                                .px(px(T::S4))
                                .rounded(px(F::BADGE / 2.))
                                .bg(Status::Waiting.colors().0)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(F::BADGE_TEXT))
                                .line_height(px(F::BADGE))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(T::color(T::WHITE))
                                .child(need.to_string()),
                        )
                    })
                    .tooltip(super::diagnostics::tip(vec![if missing {
                        "目录不在了，点一下重新选择".to_owned()
                    } else {
                        format!("{} · {}", space.name, space.cwd)
                    }]))
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
                            if missing {
                                this.choose_space_dir(Some(fix.clone()), window, cx);
                                return;
                            }
                            if event.click_count() == 2 {
                                this.rename_space(&id, window, cx);
                                return;
                            }
                            this.toggle_space_filter(&id, cx);
                        }),
                    ),
            );
        }
        // A new space is the last chip: a dashed ring with a plus.
        chips = chips.child(
            div()
                .id("add-space")
                .role(gpui::accesskit::Role::Button)
                .aria_label("新建空间")
                .size(px(F::CHIP))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border(px(T::BORDER_WIDTH))
                .border_dashed()
                .border_color(T::color(F::chip_on_line()))
                .cursor_pointer()
                .hover(|s| s.bg(T::color(F::chip_on())))
                .text_size(px(T::SMALL))
                .text_color(T::color(F::third()))
                .child("+")
                .on_click(cx.listener(|this, _, window, cx| this.new_space(window, cx))),
        );
        out.push(chips.into_any_element());
        // Narrowed to one space: what it is, its folder's branch, and the way back.
        if let Some(space) = filter
            .as_deref()
            .and_then(|f| self.workspace.spaces.iter().find(|s| s.id == f))
        {
            let branch = self.git_branch(&space.cwd, cx);
            let counts = self
                .git_divergence(&space.cwd, cx)
                .and_then(super::divergence::divergence_label);
            out.push(
                div()
                    .mt(px(T::S10))
                    .mx(px(T::S12))
                    .px(px(T::S10))
                    .py(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .rounded(px(T::S10))
                    .bg(T::color(F::scope()))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(F::chip_line()))
                    .text_size(px(T::SMALL))
                    .line_height(px(F::LINE_TEXT))
                    .child(self.space_dot(&space.id, T::S12))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(space.name.clone()),
                    )
                    .when_some(branch, |d, branch| {
                        d.child(
                            label(branch)
                                .min_w_0()
                                .truncate()
                                .text_size(px(T::CHIP_SIZE))
                                .text_color(T::color(T::muted())),
                        )
                    })
                    .when_some(counts, |d, counts| {
                        d.child(
                            label(counts)
                                .flex_shrink_0()
                                .text_size(px(T::CHIP_SIZE))
                                .text_color(T::color(T::muted()))
                                .font_features(T::tabular()),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        button("space-filter-clear", "✕")
                            .aria_label("看全部空间")
                            .flex_shrink_0()
                            .h(px(T::ICON_HEIGHT))
                            .px(px(T::S6))
                            .border_0()
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar.space_filter = None;
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }
        out
    }
    /// The space 会话优先 is narrowed to, while that space still exists.
    pub(super) fn space_filter(&self) -> Option<&str> {
        self.sidebar
            .space_filter
            .as_deref()
            .filter(|f| self.workspace.spaces.iter().any(|s| s.id == *f))
    }
    /// Narrows the list to a space and shows it, or lists every space again.
    pub(super) fn toggle_space_filter(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.space_filter() == Some(id) {
            self.sidebar.space_filter = None;
            cx.notify();
            return;
        }
        self.sidebar.space_filter = Some(id.to_owned());
        self.pick_space(id, cx);
    }
    /// A space's two colors on a round dot, as the 鸟瞰 board shows spaces.
    pub(super) fn space_dot(&self, id: &str, size: f32) -> gpui::Div {
        let [from, to] = T::space_tile(self.space_mesh(id));
        div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_full()
            .bg(gpui::linear_gradient(
                135.,
                gpui::linear_color_stop(T::color(from), 0.),
                gpui::linear_color_stop(T::color(to), 1.),
            ))
    }
    /// A session row's space: its dot and name, quiet, at the row's end.
    pub(super) fn space_tag(&self, id: &str, name: &str) -> gpui::Div {
        use crate::ui::theme::Flat as F;
        div()
            .flex_shrink_0()
            .max_w(px(F::TAG_MAX))
            .flex()
            .items_center()
            .gap(px(F::CHIP_GAP))
            .child(self.space_dot(id, F::TAG_DOT))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(T::UI_FONT)
                    .text_size(px(F::TAG))
                    .line_height(px(F::LINE_SMALL))
                    .text_color(T::color(F::third()))
                    .child(name.to_owned()),
            )
    }

    /// Whether a space is the one shown; over the overview the spaces are its filter.
    fn space_active(&self, id: &str) -> bool {
        if self.nav.overview {
            self.overview.filter_space.as_deref() == Some(id)
        } else {
            self.nav.space == id
        }
    }
    /// A space's open sessions, how many wait on you or failed, and the state its dot shows.
    fn space_counts(&self, id: &str) -> SpaceCounts {
        let sessions: Vec<_> = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| {
                self.session_hidden(s).is_none()
                    && self.session_open(s)
                    && session_space(&self.workspace, s) == Some(id)
            })
            .collect();
        let needs = |g: Status| {
            sessions
                .iter()
                .filter(|s| {
                    s.managed()
                        && s.outcome.needs_review(now())
                        && status_of(&self.workspace, s) == g
                })
                .count()
        };
        let (waiting, errors) = (needs(Status::Waiting), needs(Status::Error));
        let group = if errors > 0 {
            Status::Error
        } else if waiting > 0 {
            Status::Waiting
        } else {
            sessions
                .iter()
                .filter(|s| s.outcome.needs_review(now()))
                .map(|s| status_of(&self.workspace, s))
                .find(|g| !matches!(g, Status::Waiting | Status::Error))
                .unwrap_or(Status::Stale)
        };
        SpaceCounts {
            count: sessions.len(),
            waiting,
            errors,
            group,
        }
    }
    /// Shows a space: its first tab, or over the overview, its sessions only.
    pub(super) fn pick_space(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.nav.overview {
            self.overview.filter_space = Some(id.to_owned());
        }
        self.nav.space = id.to_owned();
        self.nav.tab = self
            .workspace
            .tabs
            .iter()
            .find(|t| t.space == id)
            .map(|t| t.id.clone());
        self.sync_panes(cx);
        cx.notify();
    }
    /// The dense presets' top line: the space shown, `▾` opening the space menu (switch,
    /// add, rename, folder, remove), a badge when another space needs you, and the
    /// space's todos as a small `待办 3` that unfolds them below.
    fn space_line(&self, cx: &Context<Self>) -> gpui::Div {
        let shown = if self.nav.overview {
            self.overview.filter_space.clone()
        } else {
            Some(self.nav.space.clone())
        };
        let space = shown
            .as_ref()
            .and_then(|id| self.workspace.spaces.iter().find(|s| &s.id == id));
        let line = div()
            .flex()
            .items_center()
            .gap(px(T::S4))
            .pb(px(T::S4))
            .font_family(T::UI_FONT);
        if let Some((_, field, _)) = &self.layout.renaming_space {
            return line.child(
                div()
                    .flex_1()
                    .h(px(T::ROW))
                    .px(px(T::S8 - T::BORDER_WIDTH))
                    .rounded(px(T::CONTROL_RADIUS))
                    .bg(T::color(T::raised()))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::focus()))
                    .text_size(px(T::SMALL))
                    .child(field.clone()),
            );
        }
        let elsewhere = self
            .workspace
            .spaces
            .iter()
            .filter(|s| shown.as_deref() != Some(s.id.as_str()))
            .map(|s| self.space_counts(&s.id))
            .map(|c| c.waiting + c.errors)
            .sum::<usize>();
        let name = space.map_or_else(|| "全部空间".to_owned(), |s| s.name.clone());
        let todos = self
            .workspace
            .tasks
            .iter()
            .filter(|t| t.space == self.nav.space)
            .count();
        let open = self.tasks.todos_open;
        line.child(
            button("space-switch", "")
                .aria_label(format!("空间 {name}，点开切换或新建"))
                .min_w_0()
                .px(px(T::S8))
                .gap(px(T::S6))
                .justify_start()
                .border_0()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(T::color(T::primary()))
                        .child(name),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(T::color(T::muted()))
                        .child("▾"),
                )
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                        this.layout.space_menu = Some((this.nav.space.clone(), event.position));
                        this.layout.space_switch = true;
                        this.layout.space_removing = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
        )
        .when(elsewhere > 0, |d| {
            d.child(
                badge(Status::Error, elsewhere)
                    .flex_shrink_0()
                    .font_family(T::mono(cx).family),
            )
        })
        .child(div().flex_1())
        .child(
            button("todos-toggle", format!("待办 {todos}"))
                .aria_label(if open { "收起待办" } else { "展开待办" })
                .flex_shrink_0()
                .h(px(T::ICON_HEIGHT))
                .px(px(T::S6))
                .border_0()
                .text_size(px(T::CHIP_SIZE))
                .text_color(T::color(if open { T::primary() } else { T::secondary() }))
                .when(open, |d| d.bg(T::color(T::raised())))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.tasks.todos_open = !this.tasks.todos_open;
                    cx.notify();
                })),
        )
        .child(
            button("sidebar-new-task", "＋")
                .aria_label("新待办")
                .flex_shrink_0()
                .h(px(T::ICON_HEIGHT))
                .px(px(T::S6))
                .border_0()
                .text_size(px(T::CHIP_SIZE))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(|this, _, window, cx| this.show_quick_task(window, cx))),
        )
    }
    /// The folded column: a way back, and how many sessions in this space need you.
    /// The space's todos: what is still to be handed to an Agent, above what is running.
    /// In the dense presets the header lives in the space line (`待办 3`), so only the
    /// quick-add field and, once opened, the list come from here.
    fn todos_view(&self, space_name: &str, dense: bool, cx: &Context<Self>) -> gpui::Div {
        let mut sidebar = div().flex().flex_col();
        let todo_count = self
            .workspace
            .tasks
            .iter()
            .filter(|t| t.space == self.nav.space)
            .count();
        let open = self.tasks.todos_open;
        if dense {
            sidebar = sidebar
                .when(open || self.tasks.quick_task.is_some(), |d| d.pt(px(T::S6)))
                .children(self.quick_task_row(cx));
        } else {
            sidebar = sidebar
                .child(
                    div()
                        .id("todos-toggle")
                        .mt(px(T::S14))
                        .pt(px(T::S12))
                        .px(px(T::S8))
                        .mb(px(T::S4))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .child(
                            label(format!(
                                "{} {space_name} 的待办 · {todo_count}",
                                if open { "▾" } else { "▸" }
                            ))
                            .flex_1(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tasks.todos_open = !this.tasks.todos_open;
                            cx.notify();
                        }))
                        .child(
                            button("sidebar-new-task", "＋")
                                .aria_label("新待办")
                                .border_0()
                                .text_color(T::color(T::link()))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    // The header toggles on click; ＋ must only open it.
                                    cx.stop_propagation();
                                    this.show_quick_task(window, cx)
                                })),
                        ),
                )
                .children(self.quick_task_row(cx));
        }
        if !open {
            return sidebar;
        }
        // First open: say what the empty pane is for before asking for a task.
        if self.tasks.quick_task.is_none()
            && !self
                .workspace
                .tasks
                .iter()
                .any(|t| t.space == self.nav.space)
        {
            sidebar = sidebar.child(
                div()
                    .px(px(T::S8))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(T::S6))
                    .child(label(
                        "还没有待办。先记下要做的事，再点「开始」派给 agent。",
                    ))
                    .child(
                        button("empty-new-task", "＋ 新建待办")
                            .h(px(T::ICON_HEIGHT))
                            .px(px(T::S6))
                            .text_size(px(T::META))
                            .text_color(T::color(T::link()))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_quick_task(window, cx)),
                            ),
                    ),
            );
        }
        for task in self
            .workspace
            .tasks
            .iter()
            .filter(|t| t.space == self.nav.space)
        {
            let tid = task.id.clone();
            let start = tid.clone();
            let count = self
                .workspace
                .agent_sessions
                .iter()
                .filter(|s| s.task.as_ref() == Some(&tid))
                .count();
            // The detail's word for where it stands, with how many sessions it has.
            let status = super::task_detail::task_status(&self.workspace, task);
            let state = match status {
                mooring_core::status::TaskState::Working
                | mooring_core::status::TaskState::Settled
                | mooring_core::status::TaskState::Proven => {
                    format!("{} · {count} 个会话", status.label())
                }
                _ => status.label().to_owned(),
            };
            sidebar = sidebar.child(
                div()
                    .id(gpui::SharedString::from(format!("task-{tid}")))
                    .h(px(T::TASK_ROW))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .rounded(px(T::CONTROL_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(T::color(T::raised())))
                    .child(
                        div()
                            .size(px(T::S8))
                            .flex_shrink_0()
                            .rounded_full()
                            .border(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::link()))
                            .when(count > 0, |d| d.bg(T::color(T::accent()))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(T::MONO_SIZE))
                                    .child(task.title.clone()),
                            )
                            .child(label(state).text_size(px(T::CHIP_SIZE))),
                    )
                    .child(
                        button(
                            gpui::SharedString::from(format!("start-task-{tid}")),
                            "开始",
                        )
                        .h(px(T::ICON_HEIGHT))
                        .px(px(T::S6))
                        // Its text, not its padding, ends where the sessions' times do.
                        .mr(-px(T::S6))
                        .text_size(px(T::META))
                        .text_color(T::color(T::link()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.ask_agent(start.clone(), false, cx);
                        })),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.nav.task_detail = Some(tid.clone());
                        cx.notify();
                    })),
            );
            if let Some(picker) = self.agent_picker(&task.id, cx) {
                sidebar = sidebar.child(picker);
            }
        }
        sidebar
    }
}
/// 「新空间」, or 「新空间 2」 and on when that name is taken.
pub(super) fn new_space_name(spaces: &[Space]) -> String {
    (1..)
        .map(|i| match i {
            1 => "新空间".to_owned(),
            i => format!("新空间 {i}"),
        })
        .find(|name| !spaces.iter().any(|s| &s.name == name))
        .unwrap_or_default()
}
impl Workbench {
    /// Turns the space's name in the sidebar into a field: Enter or a click elsewhere
    /// keeps it, Esc puts the old name back.
    pub(super) fn rename_space(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(space) = self.workspace.spaces.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        self.start_space_rename(space, window, cx);
    }
    fn start_space_rename(&mut self, space: Space, window: &mut Window, cx: &mut Context<Self>) {
        self.save_space_name(cx);
        let field = cx.new(|cx| Field::new("空间名称", &space.name, cx).inline());
        field.update(cx, |f, _| f.select_all());
        let events = cx.subscribe(&field, |this, _, event, cx| match event {
            crate::ui::field::FieldEvent::Submit => {
                this.save_space_name(cx);
                this.nav.pending_focus = true;
            }
            crate::ui::field::FieldEvent::Cancel => {
                this.layout.renaming_space = None;
                this.nav.pending_focus = true;
                cx.notify();
            }
        });
        let blur = cx.on_blur(&field.focus_handle(cx), window, |this, _, cx| {
            this.save_space_name(cx)
        });
        window.focus(&field.focus_handle(cx), cx);
        self.layout.renaming_space = Some((space, field, [events, blur]));
        cx.notify();
    }
    fn save_space_name(&mut self, cx: &mut Context<Self>) {
        let Some((mut space, field, _)) = self.layout.renaming_space.take() else {
            return;
        };
        let name = field.read(cx).text.trim().to_owned();
        // An empty or unchanged name leaves the space as it was.
        if !name.is_empty() && name != space.name {
            space.name = name;
            self.edit(Edit::UpdateSpace(space), cx);
        }
        cx.notify();
    }
    /// Starts a space with no folder and shows it with its name in a field once the host
    /// has it, so it can be named at once. Its terminals start in `$HOME` until 设置目录… in its menu picks one.
    pub(super) fn new_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.layout.space_menu = None;
        let space = Space {
            id: id(),
            name: new_space_name(&self.workspace.spaces),
            cwd: String::new(),
        };
        self.edit(Edit::AddSpace(space.clone()), cx);
        self.nav.space = space.id.clone();
        self.nav.tab = None;
        self.sync_panes(cx);
        // The field lives in the space's sidebar row, which is drawn once the host's
        // reply carries the space; focused before that, it would blur and close at once.
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..40 {
                let added = this.update_in(cx, |this, window, cx| {
                    let found = this.workspace.spaces.iter().find(|s| s.id == space.id);
                    if let Some(found) = found.cloned() {
                        this.start_space_rename(found, window, cx);
                        return true;
                    }
                    false
                });
                if !matches!(added, Ok(false)) {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
            }
        })
        .detach();
    }
    /// Asks macOS for a folder: for `target` it becomes that space's directory; with no
    /// target it starts a new space named after the folder and shows it, its empty tab
    /// offering an Agent first. 重命名 in its menu changes the name later.
    pub(super) fn choose_space_dir(
        &mut self,
        target: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout.space_menu = None;
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, _, cx| {
                let cwd = dir.to_string_lossy().into_owned();
                if let Some(target) = target {
                    if let Some(space) = this.workspace.spaces.iter().find(|s| s.id == target) {
                        this.edit(
                            Edit::UpdateSpace(Space {
                                cwd,
                                ..space.clone()
                            }),
                            cx,
                        );
                    }
                    return;
                }
                let space = Space {
                    id: id(),
                    name: dir
                        .file_name()
                        .map_or_else(|| cwd.clone(), |n| n.to_string_lossy().into_owned()),
                    cwd,
                };
                this.edit(Edit::AddSpace(space.clone()), cx);
                this.nav.space = space.id.clone();
                this.nav.tab = None;
                this.sync_panes(cx);
            });
        })
        .detach();
    }
    /// From the space menu: the first press asks, the second removes the space and
    /// closes its tabs; the host refuses while it still has unsettled todos.
    pub(super) fn remove_space(&mut self, space: &str, cx: &mut Context<Self>) {
        if !self.layout.space_removing {
            self.layout.space_removing = true;
            cx.notify();
            return;
        }
        self.layout.space_removing = false;
        self.layout.space_menu = None;
        if self.nav.space == space
            && let Some(other) = self.workspace.spaces.iter().find(|s| s.id != space)
        {
            self.nav.space = other.id.clone();
            self.nav.tab = self
                .workspace
                .tabs
                .iter()
                .find(|t| t.space == other.id)
                .map(|t| t.id.clone());
            self.sync_panes(cx);
        }
        self.edit(Edit::RemoveSpace { id: space.into() }, cx);
        cx.notify();
    }
    /// A space's right-click menu.
    pub(super) fn space_menu_view(
        &self,
        id: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let rename = id.to_owned();
        let folder = id.to_owned();
        let remove = id.to_owned();
        let tabs = self.workspace.tabs.iter().filter(|t| t.space == id).count();
        let last = self.workspace.spaces.len() <= 1;
        // From the dense presets' space line: every space to switch to, and a new one.
        let mut picks = vec![];
        if self.layout.space_switch {
            for space in &self.workspace.spaces {
                let pick = space.id.clone();
                let SpaceCounts {
                    count,
                    waiting,
                    errors,
                    group,
                } = self.space_counts(&space.id);
                picks.push(
                    button(
                        gpui::SharedString::from(format!("space-pick-{}", space.id)),
                        "",
                    )
                    .aria_label(space.name.clone())
                    .w_full()
                    .px(px(T::S8))
                    .gap(px(T::S8))
                    .justify_start()
                    .border_0()
                    .when(self.space_active(&space.id), |d| {
                        d.bg(T::color(T::button_hover()))
                    })
                    .child(dot(group))
                    .child(div().min_w_0().truncate().child(space.name.clone()))
                    .when(space_dir_missing(space), |d| {
                        d.child(
                            label("目录不在了")
                                .flex_shrink_0()
                                .text_size(px(T::CHIP_SIZE))
                                .text_color(T::color(T::muted())),
                        )
                    })
                    .child(div().flex_1())
                    .when(waiting > 0, |d| d.child(badge(Status::Waiting, waiting)))
                    .when(errors > 0, |d| d.child(badge(Status::Error, errors)))
                    .child(
                        label(count.to_string())
                            .flex_shrink_0()
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted())),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout.space_menu = None;
                        this.pick_space(&pick, cx);
                    }))
                    .into_any_element(),
                );
            }
            picks.push(
                button("space-menu-add", "＋ 新空间")
                    .w_full()
                    .justify_start()
                    .border_0()
                    .text_color(T::color(T::link()))
                    .on_click(cx.listener(|this, _, window, cx| this.new_space(window, cx)))
                    .into_any_element(),
            );
        }
        let name = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let has_dir = self
            .workspace
            .spaces
            .iter()
            .any(|s| s.id == id && !s.cwd.is_empty());
        let mut entries = picks;
        entries.extend([
            button(
                "space-menu-rename",
                if self.layout.space_switch {
                    format!("重命名「{name}」")
                } else {
                    "重命名".to_owned()
                },
            )
            .w_full()
            .justify_start()
            .border_0()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.layout.space_menu = None;
                this.rename_space(&rename, window, cx);
            }))
            .into_any_element(),
            button(
                "space-menu-folder",
                if has_dir {
                    "更换目录…"
                } else {
                    "设置目录…"
                },
            )
            .w_full()
            .justify_start()
            .border_0()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.choose_space_dir(Some(folder.clone()), window, cx);
            }))
            .into_any_element(),
            // Two presses: the first says what goes with it, the second removes it.
            button(
                "space-menu-remove",
                if !self.layout.space_removing {
                    "删除空间".to_owned()
                } else if tabs > 0 {
                    format!("确认删除，并关掉 {tabs} 个标签")
                } else {
                    "确认删除".to_owned()
                },
            )
            .w_full()
            .justify_start()
            .border_0()
            .text_color(Tone::Danger.ink())
            .when(last, |d| d.opacity(0.4))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !last {
                    this.remove_space(&remove, cx);
                }
            }))
            .into_any_element(),
        ]);
        self.popup(position, entries, window, cx, |this| {
            this.layout.space_menu = None
        })
    }
}
