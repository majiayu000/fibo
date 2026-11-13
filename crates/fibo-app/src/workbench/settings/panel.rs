//! 设置's panel: the nav column and the page beside it, and how the workbench opens it.
use super::*;

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = !self.query(cx).is_empty();
        if !self.market.loaded()
            && if searching {
                self.page_matches(Page::Plugins, cx)
            } else {
                self.page == Page::Plugins
            }
        {
            let scroll = self.market.scroll.clone();
            self.market = plugins::Market::read(cx);
            self.market.scroll = scroll;
        }
        let body = self.body(cx);
        let (title, description) = if searching {
            (t!("搜索结果"), t!("在所有分类中查找。"))
        } else {
            (self.page.label(), self.page.description())
        };
        div()
            .id("settings-panel")
            .w(px(T::SETTINGS_WIDTH))
            .max_w_full()
            .h(px(PANEL_HEIGHT))
            .max_h_full()
            .flex()
            .overflow_hidden()
            .rounded(px(T::POPOVER_RADIUS))
            .bg(T::color(T::panel()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .text_color(T::color(T::body()))
            .shadow_lg()
            .track_focus(&self.focus)
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .capture_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if this.agent_menu.is_some() {
                        this.agent_menu_key(&event.keystroke.key, cx);
                        cx.stop_propagation();
                        window.prevent_default();
                    } else if this.recording.is_some() {
                        this.key(&event.keystroke, cx);
                        cx.stop_propagation();
                        window.prevent_default();
                    } else if event.keystroke.key == "escape"
                        || matches!(
                            keymap::action(&event.keystroke, cx),
                            Some(Action::Close | Action::Settings)
                        )
                    {
                        cx.emit(SettingsAction::Close);
                        cx.stop_propagation();
                        window.prevent_default();
                    } else if this.focus.is_focused(window) {
                        // Settings never forward a shortcut to a terminal behind the panel;
                        // a field holding the focus gets its keys.
                        cx.stop_propagation();
                    }
                }),
            )
            .on_key_down(|event: &gpui::KeyDownEvent, _, cx| {
                if crate::keymap::command_key(&event.keystroke.modifiers) {
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .w(px(NAV_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .bg(T::color(T::app()))
                    .border_r(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .child(
                        div()
                            .px(px(T::S16))
                            .pt(px(T::S20))
                            .pb(px(T::S12))
                            .text_size(px(T::TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .child(t!("设置")),
                    )
                    .child(
                        div()
                            .px(px(T::S12))
                            .pb(px(T::S12))
                            .child(self.query.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .px(px(T::S8))
                            .flex()
                            .flex_col()
                            .gap(px(T::S2))
                            .children(
                                Page::ALL
                                    .into_iter()
                                    .map(|page| self.nav_item(page, searching, cx)),
                            ),
                    )
                    .child(
                        div()
                            .px(px(T::S16))
                            .py(px(T::S14))
                            .border_t(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::border()))
                            .child(self.status()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px(px(T::S20 + T::S8))
                            .pt(px(T::S20))
                            .pb(px(T::S4))
                            .flex()
                            .items_start()
                            .gap(px(T::S12))
                            .child(
                                div()
                                    .flex_1()
                                    // Lets a long description (English) wrap, not run past the close button.
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(T::S4))
                                    .child(
                                        div()
                                            .text_size(px(T::TITLE))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(T::color(T::primary()))
                                            .child(title),
                                    )
                                    .child(label(description)),
                            )
                            .child(
                                button("settings-close", "✕")
                                    .aria_label(t!("关闭设置"))
                                    .w(px(T::ROW))
                                    .text_color(T::color(T::secondary()))
                                    .on_click(
                                        cx.listener(|_, _, _, cx| cx.emit(SettingsAction::Close)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("settings-body")
                            .track_scroll(&self.market.scroll)
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(T::S20 + T::S8))
                            .pb(px(T::S20 + T::S8))
                            .child(body),
                    ),
            )
    }
}

impl Workbench {
    pub(crate) fn show_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = &self.settings {
            window.focus(&view.read(cx).focus.clone(), cx);
            return;
        }
        self.search.hide();
        let values = self.workspace.settings.clone();
        let busy = self.store.busy();
        let runtime = self.store.socket().parent().map(Path::to_path_buf);
        let remote = self.store.recognition().remote.clone();
        let view = cx.new(|cx| SettingsView::new(values, busy, runtime, remote, cx));
        cx.subscribe(&view, |this, view, action, cx| match action {
            SettingsAction::Close => {
                this.settings = None;
                this.nav.pending_focus = true;
                cx.notify();
            }
            SettingsAction::HooksChanged => this.read_hooks(cx),
            SettingsAction::MachinesChanged => {
                // Dropping the old connections ends their ssh children.
                this.machines = Workbench::start_machines(cx);
                cx.notify();
            }
            SettingsAction::PluginsChanged => {
                this.plugins_read_again(cx);
                this.request(Request::ReloadPlugins, cx);
                cx.notify();
            }
            SettingsAction::PreviewComplete(effect) => {
                if this.preview_complete(*effect, cx) {
                    this.settings = None;
                    this.nav.pending_focus = true;
                } else {
                    view.update(cx, |v, _| {
                        v.text_error = Some(t!("先打开一个终端窗格，再回来试播").into());
                    });
                }
                cx.notify();
            }
            SettingsAction::Change(values) => {
                if !this.store.busy() {
                    this.edit(Edit::Settings(values.clone()), cx);
                }
            }
            SettingsAction::Remote {
                enabled,
                relay,
                key,
            } => this.request(
                Request::RemoteConfigure {
                    enabled: *enabled,
                    relay: relay.clone(),
                    key: key.clone(),
                },
                cx,
            ),
        })
        .detach();
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.settings = Some(view);
        cx.notify();
    }
    /// Opens settings on one page, as the sidebar's 「提醒」 does for notifications.
    pub(in crate::workbench) fn show_settings_page(
        &mut self,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_settings(window, cx);
        if let Some(view) = &self.settings {
            view.update(cx, |v, cx| {
                v.page = page;
                cx.notify();
            });
        }
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    pub(crate) fn visual_settings_page_name(&self, cx: &gpui::App) -> &'static str {
        self.settings.as_ref().unwrap().read(cx).page.label()
    }
    pub(crate) fn visual_settings_values(&self) -> Settings {
        self.workspace.settings.clone()
    }
    pub(crate) fn visual_settings_query(&self, cx: &gpui::App) -> Entity<Field> {
        self.settings.as_ref().unwrap().read(cx).query.clone()
    }
    /// The page labels in nav order, for a tour that shows each in turn.
    pub(crate) fn visual_settings_pages() -> Vec<&'static str> {
        Page::ALL.into_iter().map(Page::label).collect()
    }
    pub(crate) fn visual_settings_close(&mut self, cx: &mut Context<Self>) {
        self.settings = None;
        cx.notify();
    }
    pub(crate) fn visual_agent_menu_open(&self, cx: &gpui::App) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|s| s.read(cx).agent_menu.is_some())
    }
    pub(crate) fn visual_agent_menu_scroll(
        &self,
        cx: &gpui::App,
    ) -> (gpui::ScrollHandle, Option<Agent>) {
        let settings = self.settings.as_ref().unwrap().read(cx);
        (settings.agent_scroll.clone(), settings.agent_menu)
    }
    pub(crate) fn visual_settings_plugins_loaded(&self, cx: &gpui::App) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|v| v.read(cx).market.loaded())
    }
    pub(crate) fn visual_settings_open(&self) -> bool {
        self.settings.is_some()
    }
    pub(crate) fn visual_settings_show(&mut self, label: &str, cx: &mut Context<Self>) {
        let view = self.settings.clone().expect("settings open");
        view.update(cx, |v, cx| {
            // Scenes name a page by its Chinese key; a run on an English system (CI) shows
            // the English label, so either is a match.
            let shown = fibo_core::i18n::say(label);
            v.page = (Page::ALL.into_iter())
                .find(|p| p.label() == label || p.label() == shown)
                .unwrap_or_else(|| panic!("no settings page {label:?} ({shown:?})"));
            cx.notify();
        });
    }
}
