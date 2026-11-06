//! One result row in the palette list, and the 「再显示」 row under a group.
use super::*;

impl Workbench {
    pub(super) fn hit_row(
        &self,
        hit: &Hit,
        heading: Option<(&'static str, &str)>,
        row: usize,
        selected: bool,
        query: &str,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        // Lead icon, title, second line, status at the right, and the time column.
        let mut status: Option<Status> = None;
        let (lead, title, sub, right, when): (gpui::AnyElement, String, String, String, String) =
            match hit {
                Hit::NewSession => {
                    let key = crate::keymap::current(cx)
                        .chords(crate::keymap::Action::NewTab)
                        .first()
                        .map(|c| c.display())
                        .unwrap_or_default();
                    (
                        glyph("＋"),
                        "在当前空间新建会话".into(),
                        String::new(),
                        String::new(),
                        key,
                    )
                }
                Hit::NextWaiting => (
                    glyph("→"),
                    "跳到下一个等你的会话".into(),
                    String::new(),
                    format!("{} 个", self.needs_you().len()),
                    String::new(),
                ),
                Hit::More { section, hidden } => {
                    let unit = if *section == "原话" { "条" } else { "个" };
                    return self
                        .more_row(row, selected, (*hidden).min(MORE_STEP), unit, hit, cx)
                        .into_any_element();
                }
                Hit::Machine(index) => (
                    glyph("◌"),
                    self.machine_hit_title(*index),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
                Hit::Space(id) => (
                    glyph("◌"),
                    self.space_hit_title(id),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
                Hit::View(id) => {
                    let name = presets::find(id).name.clone();
                    let on = self.sidebar.preset().id == *id;
                    (
                        glyph("▤"),
                        format!("左栏视图：{name}"),
                        String::new(),
                        if on {
                            "正在用".into()
                        } else {
                            String::new()
                        },
                        String::new(),
                    )
                }
                Hit::Diagnostics => (
                    glyph("◔"),
                    "本机诊断".into(),
                    "看谁在占 CPU / GPU / 内存".into(),
                    String::new(),
                    String::new(),
                ),
                Hit::Action(i) => {
                    let action = mooring_core::action::all().get(*i);
                    (
                        glyph("▸"),
                        action.map(|a| a.title.clone()).unwrap_or_default(),
                        String::new(),
                        action
                            .map(|a| match a.run {
                                mooring_core::action::Run::Split => "在旁边分屏执行",
                                mooring_core::action::Run::Tab => "在新标签执行",
                                mooring_core::action::Run::Paste => "放进输入行，你按回车",
                            })
                            .unwrap_or_default()
                            .into(),
                        String::new(),
                    )
                }
                Hit::Task(id) => {
                    let task = self.workspace.tasks.iter().find(|t| &t.id == id);
                    (
                        task_icon().into_any_element(),
                        task.map(|t| t.title.clone()).unwrap_or_default(),
                        String::new(),
                        task.map(|t| {
                            format!("{} · {} 条验收", t.agent.executable(), t.acceptance.len())
                        })
                        .unwrap_or_default(),
                        task.and_then(|t| self.space_name(&t.space))
                            .unwrap_or_default(),
                    )
                }
                Hit::Session { id, .. } | Hit::Quote { id, .. } => {
                    let Some(s) = self.workspace.agent_sessions.iter().find(|s| &s.id == id) else {
                        return div().into_any_element();
                    };
                    let (group, _) = session_status(&self.workspace, &self.attention.unseen, s);
                    status = Some(group);
                    let when = ago(self.workspace.session_updated(s), now());
                    match hit {
                        Hit::Quote { text, .. } => (
                            glyph("“"),
                            format!("“{}”", excerpt(text, query, EXCERPT)),
                            self.session_title(s),
                            group.label().into(),
                            when,
                        ),
                        _ => {
                            let place = (self.workspace.session_space(s))
                                .and_then(|space| self.space_name(space))
                                .unwrap_or_else(|| home_relative(&s.cwd));
                            let mut sub = format!("{place} · {}", s.runtime.executable());
                            if let Some(c) = self.workspace.content.get(&s.id)
                                && !query.is_empty()
                            {
                                let said = c.asks.len();
                                let matched =
                                    c.asks.iter().filter(|q| contains(&q.text, query)).count();
                                if matched > 0 {
                                    sub.push_str(&format!(
                                        " · 你说过 {said} 句，{matched} 句含「{query}」"
                                    ));
                                }
                            }
                            (
                                glyph("❯"),
                                self.session_title(s),
                                sub,
                                group.label().into(),
                                when,
                            )
                        }
                    }
                }
            };
        let two_lines = !query.is_empty() && !sub.is_empty();
        let open = hit.clone();
        let text = if two_lines {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_color(T::color(T::primary()))
                        .child(highlighted(&title, query)),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(T::META))
                        .text_color(T::color(T::muted()))
                        .child(sub),
                )
        } else {
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(T::color(T::primary()))
                .child(highlighted(&title, query))
        };
        let line = div()
            .id(gpui::SharedString::from(format!("search-hit-{row}")))
            .mx(px(T::S6))
            .px(px(T::S10))
            .min_h(px(if two_lines {
                T::PALETTE_ROW_TWO
            } else {
                T::PALETTE_ROW
            }))
            .flex()
            .gap(px(T::S10))
            .items_center()
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .when(selected, |d| d.bg(T::color(T::palette_row())))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.search.index != row {
                    this.search.index = row;
                    cx.notify();
                }
            }))
            .on_click(
                cx.listener(move |this, _, window, cx| this.open_hit(open.clone(), window, cx)),
            )
            .child(
                div()
                    .w(px(T::PALETTE_ICON))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(lead),
            )
            .child(text)
            .when(!right.is_empty(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(T::S4))
                        .text_size(px(T::META))
                        .text_color(T::color(T::secondary()))
                        .when_some(status, |d, g| d.child(dot(g)))
                        .child(right),
                )
            })
            .child(
                div()
                    .w(px(T::PALETTE_WHEN))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .truncate()
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .font_features(T::tabular())
                    .map(|d| match hit {
                        Hit::NewSession if !when.is_empty() => d.child(kbd(when)),
                        _ => d.child(when),
                    }),
            );
        div()
            .flex()
            .flex_col()
            .when_some(heading, |d, (heading, note)| {
                d.child(section_heading(heading, note))
            })
            .child(line)
            .into_any_element()
    }
    fn more_row(
        &self,
        row: usize,
        selected: bool,
        next: usize,
        unit: &str,
        hit: &Hit,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let open = hit.clone();
        div()
            .id(gpui::SharedString::from(format!("search-hit-{row}")))
            .mx(px(T::S6))
            .mt(px(T::S2))
            .h(px(T::PALETTE_MORE))
            .pl(px(T::S10 + T::PALETTE_ICON + T::S10))
            .flex()
            .items_center()
            .rounded(px(T::CONTROL_RADIUS))
            .text_size(px(T::SMALL))
            .text_color(T::color(T::link()))
            .font_features(T::tabular())
            .cursor_pointer()
            .when(selected, |d| d.bg(T::color(T::palette_row())))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.search.index != row {
                    this.search.index = row;
                    cx.notify();
                }
            }))
            .on_click(
                cx.listener(move |this, _, window, cx| this.open_hit(open.clone(), window, cx)),
            )
            .child(format!("再显示 {next} {unit} ↓"))
    }
}
