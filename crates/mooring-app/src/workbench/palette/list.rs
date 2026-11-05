//! The palette's panel: the query field, the status counts and the rows of results.
use super::super::overview::solid;
use super::parts::{glyph, kbd, section_heading};
use super::preview::highlighted;
use super::*;
use mooring_core::sidebar as presets;

impl Workbench {
    pub(in crate::workbench) fn search_palette(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let query = self.search_query(cx);
        let sections = self.search_sections(&query);
        let total: usize = sections.iter().map(|s| s.hits.len()).sum();
        let index = self.search.index.min(total.saturating_sub(1));
        let selected = sections.iter().flat_map(|s| &s.hits).nth(index).cloned();
        let sessions = self.searchable().len();
        // Each row is one direct child of the list, so the scroll handle can reveal it.
        let mut list = div()
            .id("search-list")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.search.scroll)
            .pt(px(T::S6))
            .pb(px(T::S8))
            .flex()
            .flex_col();
        let mut row = 0;
        for section in &sections {
            for (i, hit) in section.hits.iter().enumerate() {
                let heading = section
                    .title
                    .filter(|_| i == 0)
                    .map(|t| (t, section.note.as_str()));
                list = list.child(self.hit_row(hit, heading, row, row == index, &query, cx));
                row += 1;
            }
        }
        let preview = match &selected {
            Some(Hit::Session { id, quote }) => self.hit_preview(id, quote.as_deref(), &query, cx),
            Some(Hit::Quote { id, text }) => self.hit_preview(id, Some(text), &query, cx),
            _ => None,
        };
        let count = |title: &str| {
            sections
                .iter()
                .find(|s| s.title == Some(title))
                .map_or(0, |s| s.total)
        };
        let found = if query.is_empty() {
            None
        } else if total == 0 {
            Some("0 项".to_owned())
        } else {
            Some(format!(
                "会话 {} · 原话 {} · 待办 {}",
                count("会话"),
                count("原话"),
                count("待办")
            ))
        };
        let field_row = div()
            .h(px(T::SEARCH_FIELD))
            .flex_none()
            .px(px(T::S16))
            .flex()
            .items_center()
            .gap(px(T::S10))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .child(
                div()
                    .text_size(px(T::SEARCH_TEXT))
                    .text_color(T::color(T::secondary()))
                    .child("⌕"),
            )
            .child(div().flex_1().min_w_0().child(self.search.field.clone()))
            .when_some(found, |d, found| {
                d.child(label(found).font_features(T::tabular()))
            })
            .child(kbd("esc"));
        let nothing = !query.is_empty() && total == 0;
        let body = if nothing {
            self.no_results(&query, cx).into_any_element()
        } else {
            div()
                .h(px(T::PALETTE_LIST))
                .flex()
                .min_h_0()
                .child(list)
                .when_some(preview, |d, preview| d.child(preview))
                .into_any_element()
        };
        let keys = |key: &'static str, what: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(T::S4))
                .child(kbd(key))
                .child(what)
        };
        let footer = div()
            .flex_none()
            .px(px(T::S16))
            .py(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S14))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .text_size(px(T::META))
            .text_color(T::color(T::muted()))
            .font_features(T::tabular())
            .map(|d| {
                if nothing {
                    d.child(keys("⌫", "改关键词")).child(keys("↵", "新建会话"))
                } else {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(T::S4))
                            .child(kbd("↑"))
                            .child(kbd("↓"))
                            .child("选择"),
                    )
                    .child(keys("↵", "打开"))
                    .child(keys("⇥", "切换分组"))
                }
            })
            .child(div().flex_1())
            .child(if query.is_empty() {
                format!("只搜索本地已索引的内容 · {sessions} 个会话")
            } else {
                "只搜索本地已索引的内容".into()
            });
        let panel = div()
            .id("search-palette")
            .w(px(T::PALETTE_WIDTH))
            .max_w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(T::color(T::popover()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .rounded(px(T::PALETTE_RADIUS))
            .shadow_lg()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event, _, cx| this.search_key(event, cx)))
            .child(field_row)
            .when(query.is_empty(), |d| d.child(self.status_filters(cx)))
            .child(body)
            .child(footer);
        // Deferred so it paints after the workbench's text, which the scrim must dim too.
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(T::PALETTE_TOP))
                .px(px(T::S16))
                .bg(gpui::rgba(T::scrim()))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.close_search(cx)),
                )
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }
    /// 等你 / 出错 / 在跑 / 等你验收 with their counts; one narrows the list to that group.
    fn status_filters(&self, cx: &Context<Self>) -> gpui::Div {
        let mut row = div()
            .flex_none()
            .px(px(T::S16))
            .py(px(T::S8))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(T::S6))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .font_features(T::tabular());
        for (group, name, count) in self.status_counts() {
            let picked = self.search.group == Some(group);
            row = row.child(
                div()
                    .id(gpui::SharedString::from(format!("search-status-{name}")))
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(format!("{name} {count} 个，只看这一组"))
                    .h(px(T::FILTER_PILL))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .rounded(px(T::FILTER_PILL_RADIUS))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(if picked { T::focus() } else { T::border() }))
                    .when(picked, |d| d.bg(T::color(T::palette_row())))
                    .when(count == 0 && !picked, |d| d.border_dashed())
                    .text_size(px(T::SMALL))
                    .text_color(T::color(if count == 0 { T::muted() } else { T::body() }))
                    .cursor_pointer()
                    .hover(|d| d.bg(T::color(T::button_hover())))
                    .child(dot(group).when(count == 0, |d| d.opacity(T::ZERO_OPACITY)))
                    .child(name)
                    .child(
                        div()
                            .when(count > 0, |d| {
                                d.font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(T::color(T::primary()))
                            })
                            .child(count.to_string()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.search.group = (!picked).then_some(group);
                        this.search.shown.clear();
                        this.search.index = this.first_session_row("");
                        this.search.scroll.scroll_to_item(0);
                        cx.notify();
                    })),
            );
        }
        row.child(div().flex_1()).child(
            label(match self.search.group {
                Some(_) => "再点一下看全部",
                None => "还没收尾的会话按状态计数，点一下只看这一组",
            })
            .text_color(T::color(T::muted())),
        )
    }
    /// Nothing matched: what was searched, smaller searches that do find something, and
    /// a session to start instead.
    fn no_results(&self, query: &str, cx: &Context<Self>) -> gpui::Div {
        let visible = self.searchable();
        let asks: usize = visible
            .iter()
            .filter_map(|s| self.workspace.content.get(&s.id))
            .map(|c| c.asks.len())
            .sum();
        let tries: Vec<String> = split_terms(query)
            .into_iter()
            .filter(|t| !self.search_sections(t).is_empty())
            .collect();
        let mut suggestions = div().flex().flex_wrap().justify_center().gap(px(T::S6));
        for term in tries {
            let text = term.clone();
            suggestions = suggestions.child(
                div()
                    .id(gpui::SharedString::from(format!("search-try-{term}")))
                    .role(gpui::accesskit::Role::Button)
                    .h(px(T::FILTER_PILL))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .rounded(px(T::FILTER_PILL_RADIUS))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::body()))
                    .cursor_pointer()
                    .hover(|d| d.bg(T::color(T::button_hover())))
                    .child(format!("只搜「{term}」"))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_query(&text, cx))),
            );
        }
        div()
            .h(px(T::PALETTE_EMPTY))
            .p(px(T::S20))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(T::S12))
            .text_center()
            .child(
                div()
                    .text_size(px(T::CARD_TITLE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(T::primary()))
                    .child(format!("没有找到「{query}」")),
            )
            .child(
                div()
                    .max_w(px(T::FORM_WIDTH))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary()))
                    .child("标题、会话 ID、目录、你说过的话和待办里都没有这几个字。没被索引的旧历史不会出现在这里。"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(T::S14))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .font_features(T::tabular())
                    .child(format!("会话标题 {}", visible.len()))
                    .child(format!("原话 {asks} 句"))
                    .child(format!("待办 {}", self.workspace.tasks.len())),
            )
            .child(suggestions)
            .child(
                solid(button("search-empty-new", ""))
                    .aria_label("在当前空间新建会话")
                    .gap(px(T::S6))
                    .child("在当前空间新建会话")
                    .child(kbd("↵").text_color(T::color(T::on_accent())))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_hit(Hit::NewSession, window, cx)
                    })),
            )
    }
    fn hit_row(
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
