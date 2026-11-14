//! Drawing the docked terminals of other machines and their right-click menu: split a pane
//! right or down with an Agent or a shell in the same folder, or close it here.
use super::remote_tab::Dock;
use super::*;

impl Workbench {
    pub(super) fn dock_tree(
        &self,
        dock: &Dock,
        tree: &Layout,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        match tree {
            Layout::Leaf(key) => self.dock_leaf(dock, key, cx),
            Layout::Split {
                axis,
                first,
                second,
                ..
            } => {
                let half = |t: &Layout| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .flex()
                        .child(self.dock_tree(dock, t, cx))
                };
                div()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .when(*axis == Axis::Down, |d| d.flex_col())
                    .gap(px(T::S4))
                    .child(half(first))
                    .child(half(second))
                    .into_any_element()
            }
        }
    }
    fn dock_leaf(&self, dock: &Dock, key: &str, cx: &Context<Self>) -> gpui::AnyElement {
        let Some(peek) = dock.panes.get(key) else {
            return div().into_any_element();
        };
        let several = dock.panes.len() > 1;
        let focused = several && dock.focused == key;
        let (left, right) = (key.to_owned(), key.to_owned());
        div()
            .id(gpui::SharedString::from(format!("dock-{key}")))
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .overflow_hidden()
            .when(several, |d| {
                d.rounded(px(T::PANE_RADIUS))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(if focused { T::control() } else { T::border() }))
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| this.focus_dock_pane(&left, window, cx)),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if let Some(dock) = &mut this.dock {
                        dock.menu = Some((right.clone(), event.position, None));
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(self.peek_view(peek, cx))
            .into_any_element()
    }
    /// The open right-click menu on a docked pane, over everything.
    pub(super) fn dock_menu(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let dock = self.dock.as_ref()?;
        let (key, at, side) = dock.menu.clone()?;
        let r = dock.panes.get(&key)?.remote.as_ref()?;
        let item = |id: String, title: String| {
            button(gpui::SharedString::from(id), title)
                .w_full()
                .justify_start()
                .border_0()
        };
        let mut entries = Vec::new();
        match side {
            None => {
                let manages = r.slot.lock().unwrap().as_ref().is_some_and(|l| l.managing);
                if manages {
                    for (axis, title) in
                        [(Axis::Right, t!("向右分屏")), (Axis::Down, t!("向下分屏"))]
                    {
                        let key = key.clone();
                        entries.push(
                            item(format!("dock-split-{axis:?}"), format!("{title}…"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(dock) = &mut this.dock {
                                        dock.menu = Some((key.clone(), at, Some(axis)));
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        );
                    }
                }
                let key = key.clone();
                entries.push(
                    item("dock-close".into(), t!("关闭画面").into())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(dock) = &mut this.dock {
                                dock.menu = None;
                            }
                            this.close_dock_pane(&key, cx);
                        }))
                        .into_any_element(),
                );
            }
            Some(axis) => {
                let machine = r.machine;
                let (_, agents, first) = self.machine_agents(machine);
                let cwd = self.machine_card_cwd(r);
                let mut choices: Vec<(String, Option<Agent>)> = first
                    .into_iter()
                    .chain(agents.into_iter().filter(|a| Some(*a) != first))
                    .map(|a| (a.name().to_owned(), Some(a)))
                    .collect();
                choices.push((t!("空终端").to_owned(), None));
                for (title, agent) in choices {
                    let (key, cwd) = (key.clone(), cwd.clone());
                    entries.push(
                        item(format!("dock-split-with-{title}"), title)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(dock) = &mut this.dock {
                                    dock.menu = None;
                                    dock.split = Some((key.clone(), axis));
                                }
                                this.open_remote_shell(machine, cwd.clone(), agent, window, cx);
                            }))
                            .into_any_element(),
                    );
                }
            }
        }
        Some(self.popup(at, entries, window, cx, |this| {
            if let Some(dock) = &mut this.dock {
                dock.menu = None;
            }
        }))
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// The focused docked pane's right-click menu, on its first page or a side's.
    pub fn visual_dock_menu(&mut self, side: Option<Axis>, cx: &mut Context<Self>) {
        if let Some(dock) = &mut self.dock {
            let at = gpui::point(px(T::SIDEBAR * 2.), px(T::TOP * 4.));
            dock.menu = Some((dock.focused.clone(), at, side));
        }
        cx.notify();
    }
    /// The menu's split beside the focused pane, with `agent` or a shell.
    pub fn visual_dock_split(
        &mut self,
        axis: Axis,
        agent: Option<Agent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dock) = &mut self.dock else {
            return;
        };
        dock.menu = None;
        dock.split = Some((dock.focused.clone(), axis));
        let focused = dock.focused.clone();
        let Some(r) = self.peek_by(&focused).and_then(|p| p.remote.as_ref()) else {
            return;
        };
        let (machine, cwd) = (r.machine, self.machine_card_cwd(r));
        self.open_remote_shell(machine, cwd, agent, window, cx);
    }
    /// The docked panes in layout order, each with whether it is worked from here.
    pub fn visual_dock_panes(&self, cx: &gpui::App) -> Vec<(String, bool)> {
        let Some(dock) = &self.dock else {
            return Vec::new();
        };
        (dock.tree.ids().into_iter())
            .filter_map(|k| Some((k.clone(), !dock.panes.get(&k)?.pane.read(cx).peek)))
            .collect()
    }
}
