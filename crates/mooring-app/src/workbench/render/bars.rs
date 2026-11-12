//! The top bar's status counts and title controls, the connection notice and the
//! empty tab.
use super::*;

impl Workbench {
    /// The top bar's count per status across every open session, grouped and hidden as
    /// the overview does; one opens the overview with that group shown.
    /// The terminal style's right end of the status line: `✕ 2  ◐ 1  ✓ 3`, each a way
    /// into 鸟瞰. Zero counts are left out.
    pub(super) fn status_glyphs(&self, cx: &Context<Self>) -> gpui::Div {
        // This machine's sessions in Mooring's terminals, of those the top bar's 需要你 counts.
        let facts = self.facts();
        let counts = [self.needs_you().len(), facts.working, facts.review];
        const SHOWN: [Status; 3] = [Status::Error, Status::Running, Status::Review];
        let mut row = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(T::S10))
            .font_features(T::tabular());
        for (group, count) in SHOWN.into_iter().zip(counts).filter(|(_, n)| *n > 0) {
            row = row.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "status-{}",
                        group.label()
                    )))
                    .flex()
                    .gap(px(T::S4))
                    .cursor_pointer()
                    .child(glyph(group))
                    .child(count.to_string())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if group == Status::Running {
                            this.overview.show_running = true;
                        }
                        this.show_overview(cx);
                    })),
            );
        }
        row
    }
    /// The sidebar button beside the traffic lights: on the sidebar's top row while it is
    /// open, on the main title row once it is folded away, so it never moves.
    pub(in crate::workbench) fn sidebar_toggle(
        &self,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        // T3 Code's mark: the panel with a chevron saying which way a click moves it.
        icon_button(
            "sidebar-toggle",
            if self.sidebar.collapsed {
                "icons/sidebar-open.svg"
            } else {
                "icons/sidebar.svg"
            },
            if self.sidebar.collapsed {
                t!("展开左栏")
            } else {
                t!("收起左栏")
            },
        )
        .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
    }
    /// Empty title-bar space moves the window and zooms it on a double press, as the
    /// system title bar did before the top bar replaced it.
    pub(in crate::workbench) fn title_drag_area(
        &self,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id("title-drag")
            .flex_1()
            .h_full()
            // Where the system moves the window itself; macOS goes by the handlers below.
            .window_control_area(gpui::WindowControlArea::Drag)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.title_drag = true),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.title_drag = false),
            )
            .on_mouse_move(cx.listener(|this, _, window, _| {
                if std::mem::take(&mut this.title_drag) {
                    window.start_window_move();
                }
            }))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
    }
}

/// Minimize, maximize and close, where the system leaves them to the app. Marked as window
/// controls, not given handlers: the system then treats them as its own, so hovering maximize
/// offers its layouts and a click behaves as anywhere else.
pub(super) fn window_buttons(window: &Window) -> gpui::Div {
    use gpui::WindowControlArea as Area;
    // Glyphs of the system's icon font, as its own title bars draw them.
    let maximize = if window.is_maximized() {
        "\u{e923}"
    } else {
        "\u{e922}"
    };
    let button = |id: &'static str, glyph: &'static str, area: Area| {
        div()
            .id(id)
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .w(px(T::WINDOW_BUTTON_WIDTH))
            .h_full()
            .font_family("Segoe MDL2 Assets")
            .text_size(px(T::WINDOW_BUTTON_GLYPH))
            .text_color(T::color(T::secondary()))
            .window_control_area(area)
            .child(glyph)
    };
    div()
        .flex()
        .flex_shrink_0()
        .h_full()
        .child(
            button("window-minimize", "\u{e921}", Area::Min)
                .hover(|d| d.bg(T::color(T::selection()))),
        )
        .child(
            button("window-maximize", maximize, Area::Max)
                .hover(|d| d.bg(T::color(T::selection()))),
        )
        .child(button("window-close", "\u{e8bb}", Area::Close).hover(|d| {
            d.bg(gpui::rgb(T::WINDOW_CLOSE_HOVER))
                .text_color(gpui::white())
        }))
}

impl Workbench {
    /// One line under the tabs for a connection problem: a sentence the user can act on,
    /// the raw errors on hover, and 「重新连接」 when the host itself is unreachable.
    pub(super) fn notice_strip(
        &self,
        text: &str,
        details: Vec<String>,
        reconnect: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (_, ink, bg) = Tone::Warn.colors();
        let details: Vec<String> = details.into_iter().filter(|d| !d.is_empty()).collect();
        let message = div()
            .id("connection-notice")
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(T::S8))
            .child(
                div()
                    .size(px(T::DOT))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(Tone::Warn.dot()),
            )
            .child(div().truncate().child(text.to_string()))
            .when(!details.is_empty(), |d| {
                d.tooltip(diagnostics::tip(details))
            });
        div()
            .flex()
            .items_center()
            .gap(px(T::S8))
            .px(px(T::S14))
            .py(px(T::S2))
            .bg(bg)
            .text_size(px(T::META))
            .text_color(ink)
            .child(message)
            .child(div().flex_1())
            .when(reconnect, |d| {
                let text = if self.reconnecting {
                    t!("连接中…")
                } else {
                    t!("重新连接")
                };
                d.child(
                    button("reconnect-host", text)
                        .flex_shrink_0()
                        .text_color(ink)
                        .on_click(cx.listener(|this, _, _, cx| this.reconnect(cx))),
                )
            })
    }
    /// Starts a host when none answers; the status link then resubscribes on its own.
    pub(in crate::workbench) fn reconnect(&mut self, cx: &mut Context<Self>) {
        if self.reconnecting {
            return;
        }
        self.reconnecting = true;
        cx.notify();
        let socket = self.store.socket().to_owned();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if crate::lifecycle::connected_host(&socket)? {
                        return Ok(());
                    }
                    let root = socket
                        .parent()
                        .ok_or_else(|| std::io::Error::other(t!("找不到运行目录")))?;
                    crate::lifecycle::spawn_host(root, false)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.reconnecting = false;
                if let Err(e) = result {
                    this.error = Some(t!("没能启动后台：{e}", e));
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Workbench {
    /// A new terminal in the shown space with `agent` started in it.
    pub(in crate::workbench) fn run_agent_here(&mut self, agent: Agent, cx: &mut Context<Self>) {
        self.tasks.last_agent = Some(agent);
        self.tasks.chosen_agent = Some(agent);
        self.launch(false, None, cx);
        // Its hook may have been installed or removed from a terminal since the last read.
        self.read_hooks(cx);
    }
    /// The Agent an empty tab offers first: the last one picked, else the default.
    /// A tab with no terminal yet, a new space's first view: where work starts, one solid
    /// button for the Agent used last, the other Agents and a bare shell as quiet links.
    pub(super) fn empty_tab(&self, cx: &Context<Self>) -> gpui::Div {
        let space = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == self.nav.space);
        let first = self.first_agent();
        // Each link is its mark and its name in one piece, so a wrapped row never starts
        // or ends on a separator as the old `·` between names did.
        let link = |id: String, text: String, mark: Option<gpui::AnyElement>| {
            marked_button(id, text, mark)
                .px(px(T::S4))
                .text_color(T::color(T::link()))
        };
        let mark = |agent: Agent, ink| super::super::board::agent_mark_in(agent, ink, T::SMALL);
        let mut others = vec![];
        for agent in Agent::all().into_iter().filter(|a| *a != first) {
            others.push(
                link(
                    format!("empty-tab-{}", agent.executable()),
                    agent.name().to_owned(),
                    Some(mark(agent, T::color(T::link()))),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.run_agent_here(agent, cx)))
                .into_any_element(),
            );
        }
        others.push(
            link("empty-tab-new".into(), t!("空终端").into(), None)
                .on_click(cx.listener(|this, _, _, cx| this.launch(false, None, cx)))
                .into_any_element(),
        );
        let split = crate::keymap::current(cx)
            .chords(crate::keymap::Action::SplitRight)
            .first()
            .map(|c| c.display());
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(T::S12))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(T::S4))
                    .child(
                        div()
                            .text_size(px(T::CARD_TITLE))
                            .text_color(T::color(T::primary()))
                            .child(match space {
                                Some(s) => t!("在 {name} 里开始", name = s.name),
                                None => t!("在这里开始").to_owned(),
                            }),
                    )
                    .when_some(space, |d, s| {
                        d.child(label(super::super::space_dir_label(&self.workspace, s)))
                    }),
            )
            .children(space.and_then(|s| self.recent_dirs_view(s, cx)))
            .child(
                super::super::overview::solid(marked_button(
                    format!("empty-tab-{}", first.executable()),
                    t!("运行 {name}", name = first.name()),
                    Some(mark(first, T::color(T::on_accent()))),
                ))
                .px(px(T::S16))
                .on_click(cx.listener(move |this, _, _, cx| this.run_agent_here(first, cx))),
            )
            .child(
                div()
                    .max_w(px(T::HERE_WIDTH * 1.5))
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .items_center()
                    .gap_x(px(T::S12))
                    .gap_y(px(T::S4))
                    .text_size(px(T::SMALL))
                    .children(others),
            )
            .when_some(split, |d, keys| {
                d.child(label(t!("打开后可以右键分屏，或按 {keys} 向右分屏", keys)))
            })
    }
}

/// `widgets::button` with a CLI's mark before its words.
pub(in crate::workbench) fn marked_button(
    id: String,
    text: String,
    mark: Option<gpui::AnyElement>,
) -> gpui::Stateful<gpui::Div> {
    let text = gpui::SharedString::from(text);
    div()
        .id(gpui::SharedString::from(id))
        .role(gpui::accesskit::Role::Button)
        .aria_label(text.clone())
        .h(px(T::ROW))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(T::S6))
        .rounded(px(T::CONTROL_RADIUS))
        .text_size(px(T::SMALL))
        .cursor_pointer()
        .hover(|s| s.bg(T::color(T::button_hover())))
        .when_some(mark, |d, m| d.child(m))
        .child(text)
}
