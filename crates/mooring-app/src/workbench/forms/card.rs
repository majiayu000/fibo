//! The todo form card: fields, space choice and the save buttons.
use super::*;

impl Workbench {
    pub(super) fn form_card(
        &self,
        form: &Form,
        checks: &FormChecks,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let error = Tone::Danger.ink();
        let title_bad = form.tried && checks.title_missing;
        let card = div()
            .flex_1()
            .min_w_0()
            .p(px(T::S20))
            .flex()
            .flex_col()
            .gap(px(T::S16))
            .bg(T::color(T::popover()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .rounded(px(T::POPOVER_RADIUS))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(T::S10))
                    .child(
                        div()
                            .text_size(px(T::FORM_TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(if self.tasks.editing_task.is_some() {
                                "编辑待办"
                            } else {
                                "新建待办"
                            }),
                    )
                    .child(hint("保存后出现在左栏「待办」和鸟瞰顶部"))
                    .child(div().flex_1())
                    .child(
                        button("form-close", "×")
                            .w(px(T::ICON_WIDTH))
                            .h(px(T::ICON_WIDTH))
                            .px_0()
                            .text_color(T::color(T::muted()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_form();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(T::S6))
                    .child(form_label("要做什么", None, None))
                    .child(
                        div()
                            .rounded(px(T::CONTROL_RADIUS))
                            .when(title_bad, |d| {
                                d.border(px(T::BORDER_WIDTH)).border_color(error)
                            })
                            .child(form.fields[0].clone()),
                    )
                    .when(title_bad, |d| d.child(hint("先写要做什么").text_color(error))),
            )
            .child(self.form_spaces(form, checks, cx))
            .child(form.fields[1].clone())
            .child(self.form_criteria(form, window, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(T::S6))
                    .child(form_label("默认用哪个 Agent", Some("开始时还能改"), None))
                    .child(Agent::all().into_iter().fold(segments(), |row, agent| {
                        row.child(
                            segment(
                                gpui::SharedString::from(format!("agent-{}", agent.executable())),
                                form.agent == agent,
                            )
                            .aria_label(agent.name())
                            .child(agent.name())
                            .child(
                                div()
                                    .font(T::mono(cx))
                                    .text_size(px(T::META))
                                    .text_color(T::color(T::muted()))
                                    .child(agent.executable()),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(f) = &mut this.tasks.form {
                                    f.agent = agent;
                                }
                                cx.notify();
                            })),
                        )
                    }))
                    .when(checks.agent_missing, |d| {
                        d.child(
                            hint(format!(
                                "找不到 {} 命令：开始后会停在「找不到命令」。先安装或加进 PATH，或换一个 Agent",
                                form.agent.executable()
                            ))
                            .text_color(error),
                        )
                    }),
            );
        let blocked = checks.title_missing;
        let no_start = blocked || checks.dir_missing;
        let save = button("save", "保存为待办");
        let start = button("save-start", "")
            .gap(px(T::S6))
            .child("保存并开始")
            .child(
                div()
                    .px(px(T::S4))
                    .rounded(px(T::CHIP_RADIUS))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::on_accent()))
                    .opacity(T::KBD_OPACITY)
                    .text_size(px(T::KBD_SIZE))
                    .child("⌘⏎"),
            )
            .aria_label("保存并开始");
        card.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(T::S8))
                .pt(px(T::S16))
                .border_t(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .child(
                    hint("都能再编辑；验收标准改字不会丢掉已挂的证据")
                        .text_size(px(T::SMALL))
                        .mr_auto(),
                )
                .child(
                    button("cancel", "取消").on_click(cx.listener(|this, _, _, cx| {
                        this.close_form();
                        cx.notify();
                    })),
                )
                .child(if blocked {
                    disabled(plain(save))
                } else {
                    plain(save).on_click(cx.listener(|this, _, _, cx| this.save_form(false, cx)))
                })
                .child(if no_start {
                    disabled(solid(start))
                } else {
                    solid(start).on_click(cx.listener(|this, _, _, cx| this.save_form(true, cx)))
                }),
        )
    }
    /// The space choice; a space whose directory is gone says so and offers to repoint it.
    fn form_spaces(&self, form: &Form, checks: &FormChecks, cx: &Context<Self>) -> gpui::Div {
        let error = Tone::Danger.ink();
        let cwd = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == form.space)
            .map(space_dir_label)
            .unwrap_or_default();
        let mut row = segments();
        for space in &self.workspace.spaces {
            let sid = space.id.clone();
            let missing = space_dir_missing(space);
            row = row.child(
                segment(
                    gpui::SharedString::from(format!("project-{sid}")),
                    form.space == sid,
                )
                .aria_label(space.name.clone())
                .child(space.name.clone())
                .when(missing, |d| {
                    d.child(
                        div()
                            .px(px(T::S4))
                            .rounded(px(T::CHIP_RADIUS))
                            .bg(T::color(T::chip()))
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted()))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .child("目录不在了"),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(f) = &mut this.tasks.form {
                        f.space = sid.clone();
                    }
                    cx.notify();
                })),
            );
        }
        let space = form.space.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(T::S6))
            .child(form_label(
                "在哪个空间做",
                None,
                Some("开始时在这个目录开终端".into()),
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(T::S12))
                    .child(row)
                    .child(
                        hint(cwd)
                            .font(T::mono(cx))
                            .min_w_0()
                            .truncate()
                            .when(checks.dir_missing, |d| d.text_color(error)),
                    ),
            )
            .when(checks.dir_missing, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        .child(
                            hint("这个空间的目录不在了：可以先保存为待办，改好目录再开始")
                                .text_color(error),
                        )
                        .child(
                            button("form-space-dir", "改空间目录")
                                .h(px(T::ICON_HEIGHT))
                                .px(px(T::S6))
                                .text_color(T::color(T::link()))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.choose_space_dir(Some(space.clone()), window, cx)
                                })),
                        ),
                )
            })
    }
}
