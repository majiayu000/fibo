//! The ⌘P search palette: a floating panel over the workbench with grouped results
//! on the left and the selected session's preview on the right
//! (Keepline repo, specs/terminal-workbench/design/palette-here.html).
use super::overview::{quiet, solid};
use super::*;
use mooring_core::sidebar as presets;

mod parts;
mod preview;
mod rows;
mod search;
use parts::section_heading;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Hit {
    NewSession,
    NextWaiting,
    /// A session; `quote` is the latest of the user's own words that matched too.
    Session {
        id: String,
        quote: Option<String>,
    },
    /// The user's own words in a session whose title did not match.
    Quote {
        id: String,
        text: String,
    },
    Task(String),
    /// A plugin action, by its index in `mooring_core::action::all()`, run for the
    /// focused pane.
    Action(usize),
    /// A left-column view (目录, a plugin's), by preset id: picking it switches.
    View(String),
    /// 先不管 or 接着管 another machine, by its index in `Workbench::machines`.
    Machine(usize),
    /// 先不管 or 接着管 a space of this machine.
    Space(String),
    /// 本机诊断: opens the CPU / GPU panel, otherwise only reachable from the sidebar foot.
    Diagnostics,
    /// 「再显示 N 个」: the rest of a folded group, `hidden` rows past what shows.
    More {
        section: &'static str,
        hidden: usize,
    },
}
pub(super) struct Section {
    pub title: Option<&'static str>,
    /// What the heading says at its right: the count, sometimes with a note.
    pub note: String,
    /// Every match, including the ones folded away behind 「再显示」.
    pub total: usize,
    pub hits: Vec<Hit>,
}
/// The palette's own state: whether it shows, its query field and the selected row.
pub(super) struct Search {
    open: bool,
    field: Entity<Field>,
    /// The selected row in the palette, counted across all groups.
    index: usize,
    scroll: gpui::ScrollHandle,
    /// The status count picked above an empty query: only that group is listed.
    group: Option<Status>,
    /// Rows a group shows past its first few, by group title, after 「再显示」.
    shown: HashMap<&'static str, usize>,
}
impl Search {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let search = cx.new(|cx| {
            Field::new("搜索会话、待办或你说过的话", "", cx)
                .compact()
                .bare()
        });
        let mut last_query = String::new();
        cx.observe(&search, move |this, field, cx| {
            let query = field.read(cx).text.clone();
            if query != last_query {
                last_query = query;
                this.search.shown.clear();
                // A status pick narrows the recent list; typing searches everything.
                if !last_query.trim().is_empty() {
                    this.search.group = None;
                }
                this.search.index = this.first_session_row(last_query.trim());
                this.search.scroll.scroll_to_item(0);
            }
            cx.notify();
        })
        .detach();
        cx.subscribe_in(&search, window, |this, _, event, window, cx| match event {
            crate::ui::field::FieldEvent::Submit => this.open_selected_hit(window, cx),
            crate::ui::field::FieldEvent::Cancel => this.close_search(cx),
            crate::ui::field::FieldEvent::ClickOut => {}
        })
        .detach();
        Self {
            open: false,
            field: search,
            index: 0,
            scroll: gpui::ScrollHandle::new(),
            group: None,
            shown: HashMap::new(),
        }
    }
    pub(super) fn is_open(&self) -> bool {
        self.open
    }
    /// Hides the palette without the refocus that closing it from its own keys does.
    pub(super) fn hide(&mut self) {
        self.open = false;
    }
    #[cfg(feature = "visual-test")]
    pub(super) fn field(&self) -> &Entity<Field> {
        &self.field
    }
    #[cfg(feature = "visual-test")]
    /// As a click on a status count: a picked group lists no actions, so its first
    /// session is the first row.
    pub(super) fn pick_group(&mut self, group: Option<Status>) {
        self.group = group;
        self.index = 0;
    }
}
/// Rows a group shows before 「再显示」, and how many more each press adds.
const COLLAPSED: usize = 6;
const MORE_STEP: usize = 20;
const EXCERPT: usize = 90;
/// The status counts above an empty query, as the top bar and 鸟瞰 name them.
const COUNTED: [Status; 4] = [
    Status::Waiting,
    Status::Error,
    Status::Running,
    Status::Review,
];
impl Workbench {
    pub(super) fn search_palette(&self, cx: &Context<Self>) -> gpui::AnyElement {
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
    fn space_name(&self, id: &str) -> Option<String> {
        self.workspace
            .spaces
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
    }
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.open = true;
        self.search.group = None;
        self.search.shown.clear();
        self.search.index = self.first_session_row("");
        self.search.field.update(cx, |f, cx| {
            f.set_text("");
            cx.notify();
        });
        window.focus(&self.search.field.focus_handle(cx), cx);
        cx.notify();
    }
    pub(super) fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search.open = false;
        self.nav.pending_focus = true;
        cx.notify();
    }
}

fn glyph(text: &'static str) -> gpui::AnyElement {
    div()
        .text_color(T::color(T::secondary()))
        .child(text)
        .into_any_element()
}
pub(super) fn kbd(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .flex_shrink_0()
        .px(px(T::S4))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::control()))
        .rounded(px(T::CHIP_RADIUS))
        .text_size(px(T::CHIP_SIZE))
        .text_color(T::color(T::secondary()))
        .child(text.into())
}
fn highlighted(text: &str, query: &str) -> gpui::StyledText {
    let style = gpui::HighlightStyle {
        background_color: Some(T::color(T::matched())),
        ..Default::default()
    };
    gpui::StyledText::new(text.to_owned())
        .with_highlights(match_ranges(text, query).into_iter().map(|r| (r, style)))
}
