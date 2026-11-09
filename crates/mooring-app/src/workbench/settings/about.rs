//! 设置 › 关于: the version, where Mooring keeps its data, and the Agent CLIs' hooks.
use super::*;

impl SettingsView {
    pub(super) fn path_row(
        &self,
        first: bool,
        title: &'static str,
        path: Option<PathBuf>,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let shown = path
            .as_ref()
            .map_or_else(|| t!("未知").into(), |p| tilde(&p.display().to_string()));
        let control = secondary(
            format!("reveal-{title}"),
            crate::ui::words::local(t!("在访达中显示")),
        )
        .when(path.is_none(), |b| b.opacity(0.5))
        .on_click(cx.listener(move |_, _, _, cx| {
            let Some(path) = &path else {
                return;
            };
            // A file is selected in its folder; a folder opens itself.
            if path.is_file() {
                cx.reveal_path(path);
            } else {
                cx.open_with_system(path);
            }
        }));
        self.file_row(first, title, shown, None, control, cx)
    }
    /// A file Mooring uses: its title, the path in mono below it, then an optional note,
    /// in the error colour when `alarm`.
    fn file_row(
        &self,
        first: bool,
        title: impl Into<gpui::SharedString>,
        shown: String,
        note: Option<(String, bool)>,
        control: impl IntoElement,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .px(px(T::S16))
            .py(px(T::S12))
            .flex()
            .items_center()
            .gap(px(T::S16))
            .when(!first, |d| {
                d.border_t(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(T::S2))
                    .child(
                        div()
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::primary()))
                            .child(title.into()),
                    )
                    .child(
                        div()
                            .truncate()
                            .font(T::mono(cx))
                            .text_size(px(T::META))
                            .text_color(T::color(T::secondary()))
                            .child(shown),
                    )
                    .when_some(note, |d, (note, alarm)| {
                        d.child(label(note).when(alarm, |l| l.text_color(Tone::Danger.ink())))
                    }),
            )
            .child(control)
            .into_any_element()
    }
    /// One row per CLI whose hooks Mooring installs: their state as `mooring hooks
    /// status` read it when the panel opened, and a button that installs them anew.
    fn hook_rows(&self, first: bool, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        use crate::agent_hooks::State;
        let hooks = match &self.hooks {
            None => {
                return vec![
                    self.row(first, "Agent Hook", t!("正在检查…"), div())
                        .into_any_element(),
                ];
            }
            Some(Err(e)) => {
                let note = t!("没能检查：{e}", e);
                return vec![
                    self.row(first, "Agent Hook", &note, div())
                        .into_any_element(),
                ];
            }
            Some(Ok(hooks)) => hooks,
        };
        hooks
            .iter()
            .enumerate()
            .map(|(i, hook)| {
                let (state, action) = match hook.state {
                    State::Current => (t!("已安装且是最新"), None),
                    State::Outdated => (t!("已安装但过期"), Some(t!("重新安装"))),
                    State::Missing => (t!("未安装"), Some(t!("安装"))),
                    State::Unreadable => (t!("读不了这个文件"), None),
                };
                let mut note = state.to_owned();
                if !hook.differences.is_empty() {
                    note = t!(
                        "{note}：{diffs}",
                        note,
                        diffs = hook.differences.join(t!("；"))
                    );
                }
                let busy = self.installing.is_some();
                let control = action.map(|text| {
                    let agent = hook.agent.clone();
                    let text = if self.installing.as_ref() == Some(&agent) {
                        t!("正在安装…")
                    } else {
                        text
                    };
                    secondary(format!("hooks-{agent}"), text)
                        .when(busy, |b| b.opacity(0.5))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.installing.is_some() {
                                return;
                            }
                            // Writes the CLI's settings: only ever from this click.
                            this.installing = Some(agent.clone());
                            cx.notify();
                            Self::check_hooks(Some(agent.clone()), cx);
                        }))
                });
                self.file_row(
                    first && i == 0,
                    format!("{} Hook", hook.name),
                    tilde(&hook.settings),
                    Some((
                        note,
                        matches!(hook.state, State::Outdated | State::Unreadable),
                    )),
                    div().children(control),
                    cx,
                )
            })
            .collect()
    }
    pub(super) fn about_page(&self, cx: &Context<Self>) -> gpui::Div {
        let mut page = div().flex().flex_col();
        if self.shows(Page::About, t!("版本"), cx) {
            page = page.child(
                card().mt(px(T::S20)).child(
                    div()
                        .p(px(T::S20))
                        .flex()
                        .items_center()
                        .gap(px(T::S16))
                        .child(
                            gpui::img(cx.global::<Chosen>().icon.title_image())
                                .size(px(ICON_PREVIEW + T::S12)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(T::S4))
                                .child(
                                    div()
                                        .text_size(px(T::TITLE))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(T::color(T::primary()))
                                        .child("Mooring"),
                                )
                                .child(label(t!(
                                    "版本 {version} · Agent 工作台",
                                    version = env!("CARGO_PKG_VERSION")
                                )))
                                .child(label(t!("别再丢掉 Codex 与 Claude Code 里的工作。"))),
                        ),
                ),
            );
        }
        let config = crate::ui::status_colors::config_dir();
        let mut data = Vec::new();
        if self.shows(Page::About, t!("运行数据"), cx) {
            data.push(self.path_row(data.is_empty(), t!("运行数据"), self.runtime.clone(), cx));
        }
        if self.shows(Page::About, t!("偏好文件"), cx) {
            data.push(self.path_row(data.is_empty(), t!("偏好文件"), config.clone(), cx));
        }
        if self.shows(Page::About, "Agent Hook", cx) {
            data.extend(self.hook_rows(data.is_empty(), cx));
        }
        let updates = self.update_rows(cx);
        page.child(self.cards(vec![(t!("更新"), updates), (t!("数据与偏好"), data)]))
            .child(div().pt(px(T::S12)).px(px(T::S4)).child(label(t!(
                "工作区设置随数据库保存；外观、终端文字与快捷键存在偏好文件夹，只属于这台电脑。"
            ))))
    }
}
