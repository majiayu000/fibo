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
                "展开左栏"
            } else {
                "收起左栏"
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
                    "连接中…"
                } else {
                    "重新连接"
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
                        .ok_or_else(|| std::io::Error::other("找不到运行目录"))?;
                    crate::lifecycle::spawn_host(root, false)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.reconnecting = false;
                if let Err(e) = result {
                    this.error = Some(format!("没能启动后台：{e}"));
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
        let link = |id: String, text: String| {
            button(gpui::SharedString::from(id), text)
                .border_0()
                .px(px(T::S4))
                .text_color(T::color(T::link()))
        };
        let mut others = vec![];
        for agent in Agent::all().into_iter().filter(|a| *a != first) {
            others.push(
                link(
                    format!("empty-tab-{}", agent.executable()),
                    agent.name().to_owned(),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.run_agent_here(agent, cx)))
                .into_any_element(),
            );
        }
        others.push(
            link("empty-tab-new".into(), "空终端".into())
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
                                Some(s) => format!("在 {} 里开始", s.name),
                                None => "在这里开始".to_owned(),
                            }),
                    )
                    .when_some(space, |d, s| {
                        d.child(label(super::super::space_dir_label(s)))
                    }),
            )
            .child(
                super::super::overview::solid(button(
                    gpui::SharedString::from(format!("empty-tab-{}", first.executable())),
                    format!("运行 {}", first.name()),
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
                    .gap(px(T::S4))
                    .text_size(px(T::SMALL))
                    .children(others.into_iter().enumerate().flat_map(|(i, b)| {
                        (i > 0)
                            .then(|| label("·").into_any_element())
                            .into_iter()
                            .chain(std::iter::once(b))
                    })),
            )
            .when_some(split, |d, keys| {
                d.child(label(format!("打开后可以右键分屏，或按 {keys} 向右分屏")))
            })
    }
}
