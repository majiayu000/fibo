//! The ⌘P search palette: a floating panel over the workbench with grouped results
//! on the left and the selected session's preview on the right
//! (Keepline repo, specs/terminal-workbench/design/palette-here.html).
use super::overview::{quiet, solid};
use super::*;
use fibo_core::{
    protocol::{MessageMatch, MessagePosition},
    sidebar as presets,
};

mod commands;
mod message;
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
    Message(MessageMatch),
    Task(String),
    /// A plugin action, by its index in `fibo_core::action::all()`, run for the
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
    /// Agent Hook: opens 设置 › 通用, where a CLI's hook is installed and taken out.
    Hooks,
    /// A shortcut action (鸟瞰、新建待办、分屏…), run as its keys would.
    Command(crate::keymap::Action),
    /// A settings page, opened.
    Settings(settings::Page),
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
/// The current page and the bounds used to reach it. Older pages are fetched on demand.
struct MessagePage {
    query: String,
    hits: Vec<MessageMatch>,
    trail: Vec<MessagePosition>,
    next: Option<MessagePosition>,
}
/// The palette's own state: whether it shows, its query field and the selected row.
pub(super) struct Search {
    open: bool,
    field: Entity<Field>,
    observed_query: String,
    /// The selected row in the palette, counted across all groups.
    index: usize,
    scroll: gpui::ScrollHandle,
    /// The status count picked above an empty query: only that group is listed.
    group: Option<Status>,
    /// Rows a group shows past its first few, by group title, after 「再显示」.
    shown: HashMap<&'static str, usize>,
    message_search: Option<gpui::Task<()>>,
    message_result: Option<MessagePage>,
    message_failed: bool,
    opened_message: Option<message::OpenedMessage>,
}
impl Search {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let search = cx.new(|cx| {
            Field::new(t!("搜索会话、聊天正文或待办"), "", cx)
                .compact()
                .bare()
        });
        cx.observe(&search, |this, field, cx| {
            let query = field.read(cx).text.clone();
            if query != this.search.observed_query {
                this.search.observed_query = query.clone();
                this.search_messages(query.trim(), cx);
                this.search.shown.clear();
                // A status pick narrows the recent list; typing searches everything.
                this.search.index = if query.trim().is_empty() {
                    this.first_recent_session_row()
                } else {
                    this.search.group = None;
                    // Search results start with sessions or quotes when either matches.
                    0
                };
                this.search.scroll.scroll_to_item(0);
                // New hits: the field's own redraw covers the caret and selection.
                this.pane_changed(cx);
            }
        })
        .detach();
        cx.subscribe_in(&search, window, |this, _, event, window, cx| match event {
            crate::ui::field::FieldEvent::Submit => this.open_selected_hit(window, cx),
            crate::ui::field::FieldEvent::Cancel => {
                if !this.back_to_search_results(cx) {
                    this.close_search(cx);
                }
            }
            crate::ui::field::FieldEvent::ClickOut => {}
        })
        .detach();
        Self {
            open: false,
            field: search,
            observed_query: String::new(),
            index: 0,
            scroll: gpui::ScrollHandle::new(),
            group: None,
            shown: HashMap::new(),
            message_search: None,
            message_result: None,
            message_failed: false,
            opened_message: None,
        }
    }
    pub(super) fn is_open(&self) -> bool {
        self.open
    }
    /// Hides the palette without the refocus that closing it from its own keys does.
    pub(super) fn hide(&mut self) {
        self.open = false;
        self.message_search = None;
        self.opened_message = None;
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
        let reading = self.search.opened_message.is_some();
        let sections = if reading {
            Vec::new()
        } else {
            self.search_sections(&query)
        };
        let total: usize = sections.iter().map(|s| s.hits.len()).sum();
        let index = self.search.index.min(total.saturating_sub(1));
        let selected = sections.iter().flat_map(|s| &s.hits).nth(index).cloned();
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
            Some(hit @ Hit::Session { id, quote }) => {
                self.hit_preview(id, quote.as_deref(), &query, hit.clone(), cx)
            }
            Some(open @ Hit::Message(hit)) => self.hit_preview(
                &hit.position.session,
                Some(&hit.text),
                &query,
                open.clone(),
                cx,
            ),
            _ => None,
        };
        let count = |title: &str| {
            sections
                .iter()
                .find(|s| s.title == Some(title))
                .map_or(0, |s| s.total)
        };
        let found = if query.is_empty() || reading {
            None
        } else if self.search.message_search.is_some() {
            Some(t!("正在搜索聊天正文…").to_owned())
        } else if self.search.message_failed {
            Some(t!("聊天正文搜索失败").to_owned())
        } else if total == 0 {
            Some(t!("0 项").to_owned())
        } else {
            Some(t!(
                "会话 {sessions} · 消息 {messages} · 待办 {todos}",
                sessions = count(t!("会话")),
                messages = count(t!("聊天正文")),
                todos = count(t!("待办"))
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
        let nothing = !query.is_empty()
            && total == 0
            && self.search.message_search.is_none()
            && !self.search.message_failed;
        let body = if let Some(message) = self.search.opened_message.as_ref() {
            self.search_message_body(message, cx).into_any_element()
        } else if nothing {
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
                if reading {
                    d.child(keys("esc", t!("返回搜索结果")))
                } else if nothing {
                    d.child(keys("⌫", t!("改关键词")))
                        .child(keys("↵", t!("新建会话")))
                } else {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(T::S4))
                            .child(kbd("↑"))
                            .child(kbd("↓"))
                            .child(t!("选择")),
                    )
                    .child(keys("↵", t!("打开")))
                    .child(keys("⇥", t!("切换分组")))
                }
            })
            .child(div().flex_1())
            .when(!reading, |d| {
                d.when_some(self.message_page_buttons(&query, cx), |d, buttons| {
                    d.child(buttons)
                })
            })
            .child(if query.is_empty() {
                let sessions = self.searchable().len();
                t!("只搜索本地已索引的内容 · {sessions} 个会话", sessions)
            } else {
                t!("只搜索本地已索引的内容").into()
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
                    .aria_label(t!("{name} {count} 个，只看这一组", name, count))
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
                        this.search.index = this.first_recent_session_row();
                        this.search.scroll.scroll_to_item(0);
                        cx.notify();
                    })),
            );
        }
        row.child(div().flex_1()).child(
            label(match self.search.group {
                Some(_) => t!("再点一下看全部"),
                None => t!("还没收尾的会话按状态计数，点一下只看这一组"),
            })
            .text_color(T::color(T::muted())),
        )
    }
    /// Nothing matched: what was searched, smaller searches that do find something, and
    /// a session to start instead.
    fn no_results(&self, query: &str, cx: &Context<Self>) -> gpui::Div {
        let visible = self.searchable();
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
                    .child(t!("只搜「{term}」", term))
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
                    .child(t!("没有找到「{query}」", query)),
            )
            .child(
                div()
                    .max_w(px(T::FORM_WIDTH))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary()))
                    .child(t!("已保存的聊天正文、会话信息和待办里都没有找到。")),
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
                    .child(t!("会话标题 {len}", len = visible.len()))
                    .child(t!("待办 {len}", len = self.workspace.tasks.len())),
            )
            .child(suggestions)
            .child(
                solid(button("search-empty-new", ""))
                    .aria_label(t!("在当前空间新建会话"))
                    .gap(px(T::S6))
                    .child(t!("在当前空间新建会话"))
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
        self.search.hide();
        self.search.open = true;
        self.search.observed_query.clear();
        self.search.message_result = None;
        self.search.message_failed = false;
        self.search.group = None;
        self.search.shown.clear();
        self.search.index = self.first_recent_session_row();
        self.search.field.update(cx, |f, cx| {
            f.set_text("");
            cx.notify();
        });
        window.focus(&self.search.field.focus_handle(cx), cx);
        cx.notify();
    }
    pub(super) fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search.hide();
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
