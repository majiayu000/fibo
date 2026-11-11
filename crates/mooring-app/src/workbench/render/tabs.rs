//! The tab strip's overflow markers and the hover notes on tabs.
use super::*;

/// A stable slot: the state and close control trade visibility, never layout space.
pub(super) fn tab_tail(
    close: gpui::Stateful<gpui::Div>,
    status: Option<Status>,
    group: gpui::SharedString,
) -> gpui::Div {
    div()
        .relative()
        .flex_shrink_0()
        .size(px(T::ICON_HEIGHT))
        .when_some(status, |d, g| {
            d.child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(T::CHIP_SIZE))
                    .group_hover(group, |s| s.opacity(0.))
                    .child(crate::ui::widgets::status_mark(g)),
            )
        })
        .child(close.absolute().size_full().px_0())
}

impl Workbench {
    #[cfg(feature = "visual-test")]
    pub fn visual_tab_agent_sample(&mut self, cx: &mut Context<Self>) {
        let tab = self
            .workspace
            .tabs
            .iter_mut()
            .find(|t| t.id == "dense")
            .unwrap();
        tab.space = self.nav.space.clone();
        tab.layout = Some(mooring_core::layout::Layout::Leaf("dense-t2".into()));
        cx.notify();
    }

    #[cfg(feature = "visual-test")]
    pub fn visual_tab_bounds(&self) -> Vec<gpui::Bounds<gpui::Pixels>> {
        (0..self.visual_tab_row().0.len())
            .filter_map(|ix| self.layout.tab_scroll.bounds_for_item(ix))
            .collect()
    }

    /// Tabs hidden past each edge of the strip. Read from the last layout, so one more
    /// frame is drawn whenever the new layout moved tabs across an edge.
    pub(super) fn tab_overflow(
        &self,
        wants: &[Option<Status>],
        cx: &mut Context<Self>,
    ) -> (Option<TabEdge>, Option<TabEdge>) {
        let (left, right) = hidden_tabs(&self.layout.tab_scroll, wants.len());
        let shown = (left.len(), right.len());
        let scroll = self.layout.tab_scroll.clone();
        let count = wants.len();
        // The display link sleeps while idle, so a next-frame callback may never come;
        // a short timer checks the fresh layout instead.
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(16))
                .await;
            let (left, right) = hidden_tabs(&scroll, count);
            if (left.len(), right.len()) != shown {
                view.update(cx, |_, cx| cx.notify()).ok();
            }
        })
        .detach();
        let edge = |mut hidden: Vec<usize>, nearest_last: bool| {
            if nearest_last {
                hidden.reverse();
            }
            let nearest = *hidden.first()?;
            let urgent = hidden.iter().copied().find(|&ix| wants[ix].is_some());
            Some(TabEdge {
                count: hidden.len(),
                reveal: urgent.unwrap_or(nearest),
                group: urgent.and_then(|ix| wants[ix]),
            })
        };
        (edge(left, true), edge(right, false))
    }
    /// The marker at one edge of the strip: how many tabs are out of sight that way,
    /// tinted when one of them is waiting on you. A click brings that tab into view.
    pub(super) fn tab_edge(
        &self,
        edge: TabEdge,
        right: bool,
        terminal_style: bool,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let text = match (right, terminal_style) {
            (true, true) => t!("还有 {count} 个 ›", count = edge.count),
            (false, true) => t!("‹ {count} 个", count = edge.count),
            (true, false) => format!("+{} ▾", edge.count),
            (false, false) => format!("‹ {}", edge.count),
        };
        let reveal = edge.reveal;
        button(
            if right {
                "tabs-more-right"
            } else {
                "tabs-more-left"
            },
            "",
        )
        .aria_label(if right {
            t!("右侧还有 {count} 个标签", count = edge.count)
        } else {
            t!("左侧还有 {count} 个标签", count = edge.count)
        })
        .flex_shrink_0()
        .h_full()
        .px(px(T::S8))
        .gap(px(T::S4))
        // Terminal style: a full-height block at the strip's edge. Standard: a small
        // count beside the faded edge, never covering a tab's name.
        .when(terminal_style, |d| {
            d.rounded_none()
                .when(right, |d| d.border_l(px(T::BORDER_WIDTH)))
                .when(!right, |d| d.border_r(px(T::BORDER_WIDTH)))
                .border_color(T::color(T::border()))
        })
        // Standard: a gray `+4 ▾` on the strip's white, with the state's dot when a hidden
        // tab needs you.
        .when(!terminal_style, |d| {
            d.font_features(T::tabular())
                .bg(T::surface(T::pane()))
                .text_color(T::color(T::muted()))
        })
        .when(terminal_style, |d| {
            d.bg(match edge.group {
                Some(g) => g.colors().2,
                None => T::color(T::raised()),
            })
            .text_color(match edge.group {
                Some(g) => g.colors().1,
                None => T::color(T::primary()),
            })
        })
        .text_size(px(T::MONO_SIZE))
        .when_some(edge.group, |d, g| d.child(dot(g)))
        .child(text)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.reveal_tab(reveal, right);
            cx.notify();
        }))
    }
    /// Scrolls the strip just far enough to show one tab whole at the given edge.
    pub(super) fn reveal_tab(&self, ix: usize, right: bool) {
        let scroll = &self.layout.tab_scroll;
        let (view, max) = (scroll.bounds(), scroll.max_offset().x);
        let Some(tab) = scroll.bounds_for_item(ix) else {
            return;
        };
        // Set directly rather than through scroll_to_item, so the next redraw already
        // counts the tabs from the new position.
        let x = if right {
            view.right() - tab.right()
        } else {
            view.left() - tab.left()
        };
        let y = scroll.offset().y;
        scroll.set_offset(gpui::point(x.clamp(-max, px(0.)), y));
    }
}

/// Tabs scrolled out of sight past one edge of the strip.
pub(super) struct TabEdge {
    count: usize,
    /// The tab a click brings into view: the nearest one waiting on you, else the nearest.
    reveal: usize,
    group: Option<Status>,
}

/// Indexes of the tabs wholly or partly past the left and right edges of the strip,
/// from its last layout. A tab cut off at the edge counts as hidden.
pub(in crate::workbench) fn hidden_tabs(
    scroll: &gpui::ScrollHandle,
    count: usize,
) -> (Vec<usize>, Vec<usize>) {
    let view = scroll.bounds();
    let shift = scroll.offset().x;
    let slack = px(T::BORDER_WIDTH);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for ix in 0..count {
        let Some(tab) = scroll.bounds_for_item(ix) else {
            break;
        };
        if tab.left() + shift < view.left() - slack {
            left.push(ix);
        } else if tab.right() + shift > view.right() + slack {
            right.push(ix);
        }
    }
    (left, right)
}

/// A plain hover note.
pub(in crate::workbench) struct TextTip(pub(in crate::workbench) gpui::SharedString);
impl Render for TextTip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(T::TAB_TIP_WIDTH))
            .p(px(T::S8))
            .rounded(px(T::CONTROL_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .bg(T::color(T::raised()))
            .shadow_lg()
            .text_size(px(T::SMALL))
            .text_color(T::color(T::primary()))
            .child(self.0.clone())
    }
}
/// A tab's full name, what its agent is doing and where, shown on hover since the tab
/// itself is capped and elided.
#[derive(Clone)]
pub(super) struct TabTip {
    title: String,
    status: Option<Status>,
    /// Why the tab asks for you: the error or the question, so the dot is never a riddle.
    reason: Option<String>,
    cwd: Option<String>,
}
impl Workbench {
    pub(super) fn tab_tip(&self, title: &str, ids: &[String]) -> TabTip {
        let sessions = self.current_on(ids);
        let (status, reason) = sessions
            .iter()
            .map(|s| {
                (
                    s,
                    session_status(&self.workspace, &self.attention.unseen, s),
                )
            })
            .max_by_key(|(_, (_, attention))| *attention)
            .map(|(s, (g, _))| {
                let line = self.attention_text(s, g).and_then(|t| {
                    let l = t.lines().map(str::trim).find(|l| !l.is_empty())?;
                    Some(l.chars().take(T::TAB_TIP_REASON_CHARS).collect::<String>())
                });
                (Some(g), line)
            })
            .unwrap_or_default();
        let first = ids
            .iter()
            .find(|id| Some(*id) == self.nav.focused.as_ref())
            .or(ids.first());
        let cwd = first
            .and_then(|id| self.workspace.terminals.iter().find(|t| &t.id == id))
            .map(|t| home_relative(&t.cwd));
        TabTip {
            title: title.to_owned(),
            status,
            reason,
            cwd,
        }
    }
    /// Why the session needs you, as a short phrase from the reasons rules (`批准
    /// WebSearch`, `要重新登录`, a question's title). `None` when no rule knows the text or
    /// the rule has nothing to add: the state word alone says it, the full text is in
    /// [`Self::attention_text`].
    pub(in crate::workbench) fn attention_reason(
        &self,
        s: &AgentSession,
        g: Status,
    ) -> Option<String> {
        use mooring_core::reason::{Kind, phrase};
        let kind = match g {
            Status::Waiting => Kind::Waiting,
            Status::Error => Kind::Error,
            _ => return None,
        };
        let text = self.attention_text(s, g)?;
        Some(phrase(kind, &text).text).filter(|p| !p.is_empty())
    }
    /// The whole text behind 等你回答 / 出错了: the question or approval, the error.
    pub(in crate::workbench) fn attention_text(
        &self,
        s: &AgentSession,
        g: Status,
    ) -> Option<String> {
        let content = self.workspace.content.get(&s.id);
        let text = match g {
            Status::Waiting => content.map(|c| c.waiting.clone()).unwrap_or_default(),
            Status::Error => content
                .map(|c| c.error.clone())
                .filter(|e| !e.is_empty())
                .or_else(|| {
                    let t = self
                        .workspace
                        .terminals
                        .iter()
                        .find(|t| s.terminal.as_ref() == Some(&t.id))?;
                    t.error
                        .clone()
                        .or_else(|| t.exit_code.map(|c| t!("终端退出码 {c}", c)))
                })
                .unwrap_or_default(),
            _ => return None,
        };
        Some(text).filter(|t| !t.trim().is_empty())
    }
}
impl Render for TabTip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(T::TAB_TIP_WIDTH))
            .p(px(T::S8))
            .flex()
            .flex_col()
            .gap(px(T::S4))
            .rounded(px(T::CONTROL_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .bg(T::color(T::raised()))
            .shadow_lg()
            .text_size(px(T::SMALL))
            .text_color(T::color(T::primary()))
            .child(self.title.clone())
            .when_some(self.status, |d, g| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S6))
                        .child(dot(g))
                        .child(label(g.label())),
                )
            })
            .when_some(self.reason.clone(), |d, reason| {
                d.child(div().text_color(T::color(T::secondary())).child(reason))
            })
            .when_some(self.cwd.clone(), |d, cwd| d.child(label(cwd)))
    }
}
