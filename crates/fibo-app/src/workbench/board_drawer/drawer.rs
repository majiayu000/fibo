//! A card's detail drawer: what the session is, what it was asked and what happened.
use super::super::board::{agent_mark, mesh};
use super::*;
use gpui::relative;

impl Workbench {
    pub(in crate::workbench) fn drawer_view(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        let (column, s) = self.drawer_session()?;
        let at = now();
        let list = self.column_sessions(column, at);
        let index = list.iter().position(|x| x.id == s.id).unwrap_or(0);
        let content = self.workspace.content.get(&s.id);
        let group = status_of(&self.workspace, s);
        let status = if column == Column::Done {
            Status::Closed.label()
        } else {
            group.label()
        };
        let said = content
            .map(|c| {
                [&c.waiting, &c.error, &c.last_reply]
                    .into_iter()
                    .find(|t| !t.trim().is_empty())
                    .map(|t| fibo_core::content::plain_line(t))
                    .unwrap_or_default()
            })
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| self.session_line(s));
        let space_id = self.workspace.session_space(s).unwrap_or(not_ui!("外部"));
        let space = self
            .workspace
            .spaces
            .iter()
            .find(|p| p.id == space_id)
            .map_or(t!("外部"), |p| p.name.as_str());
        let live = live_terminal(&self.workspace, s).is_some();
        // 要你处理 tells 等你回答 from 出错 by the status's own dot.
        let marked = column == Column::Needs;
        let status_icon = || {
            if marked {
                dot(group).into_any_element()
            } else {
                self.column_icon(column, true, cx)
            }
        };
        let open = s.clone();
        let small = |id: &'static str, text: &'static str| {
            button(id, text)
                .h(px(T::ICON_HEIGHT))
                .px(px(T::S6))
                .text_color(T::color(T::secondary()))
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(T::S8))
            .text_size(px(T::SMALL))
            .text_color(T::color(T::muted()))
            .child(status_icon())
            .child(agent_mark(s.runtime, T::color(T::secondary())))
            .child(
                div()
                    .text_color(T::color(T::secondary()))
                    .child(s.cli_session_id.chars().take(8).collect::<String>()),
            )
            .child(format!("{} / {}", index + 1, list.len()))
            .child(
                small("drawer-up", "↑")
                    .aria_label(t!("上一个"))
                    .on_click(cx.listener(|this, _, _, cx| this.step_drawer(-1, cx))),
            )
            .child(
                small("drawer-down", "↓")
                    .aria_label(t!("下一个"))
                    .on_click(cx.listener(|this, _, _, cx| this.step_drawer(1, cx))),
            )
            .child(div().flex_1())
            .child(
                small("drawer-close", "✕")
                    .aria_label(t!("关闭"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.overview.drawer = None;
                        cx.notify();
                    })),
            );
        let primary = gpui::SharedString::from(match (marked, live) {
            (true, true) => t!("打开终端回答 →"),
            (_, true) => t!("打开终端 →"),
            _ => t!("接着聊 →"),
        });
        let mut actions = div().flex().gap(px(T::S8)).child(
            super::overview::solid(button("drawer-open", primary)).on_click(cx.listener(
                move |this, _, window, cx| {
                    this.overview.drawer = None;
                    this.open_session(&open, window, cx);
                },
            )),
        );
        let move_to = match column {
            Column::Needs | Column::Review => Some((Column::Done, t!("标记完成").to_owned())),
            Column::Done => Some((
                Column::Needs,
                t!("退回{needs}", needs = fibo_core::status::needs_you_word()),
            )),
            _ => None,
        };
        if let Some((to, text)) = move_to {
            let card = DraggedCard {
                id: s.id.clone(),
                title: self.session_title(s),
                from: column,
            };
            actions = actions.child(
                super::overview::quiet(button("drawer-move", text))
                    .on_click(cx.listener(move |this, _, _, cx| this.drop_card(&card, to, cx))),
            );
        }
        let asks: Vec<gpui::AnyElement> = content
            .map(|c| c.asks.iter().rev().take(3).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|q| {
                fact_row(
                    t!("你"),
                    q.text.lines().next().unwrap_or("").to_owned(),
                    short_ago(q.at, at),
                )
            })
            .collect();
        let history: Vec<gpui::AnyElement> = self
            .workspace
            .outcome_history
            .iter()
            .rev()
            .filter(|h| h.session == s.id)
            .take(4)
            .map(|h| {
                fact_row(
                    t!("结局"),
                    format!(
                        "{} → {}{}",
                        outcome_label(&h.before),
                        outcome_label(&h.after),
                        if h.reason.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", h.reason)
                        }
                    ),
                    short_ago(h.at, at),
                )
            })
            .collect();
        let mut main = div()
            .id("drawer-main")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .px(px(T::S20))
            .py(px(T::S14))
            .flex()
            .flex_col()
            .gap(px(T::S14))
            .child(head)
            .child(
                div()
                    .text_size(px(T::TITLE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(T::primary()))
                    .child(self.session_title(s)),
            )
            // The todo it was started for; the card leaves this out to stay short.
            .children(
                (s.task.as_ref())
                    .and_then(|t| self.workspace.tasks.iter().find(|task| &task.id == t))
                    .map(|task| {
                        let id = task.id.clone();
                        div()
                            .id("drawer-todo")
                            .min_w_0()
                            .truncate()
                            .cursor_pointer()
                            .text_size(px(T::META))
                            .text_color(T::color(T::link()))
                            .child(t!("待办 · {title} ›", title = task.title))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.overview.drawer = None;
                                this.nav.task_detail = Some(id.clone());
                                cx.notify();
                            }))
                    }),
            )
            .child(
                div()
                    .p(px(T::S12))
                    .rounded(px(T::CARD_RADIUS))
                    .bg(T::color(T::popover()))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::card_border()))
                    .flex()
                    .flex_col()
                    .gap(px(T::S6))
                    .child(label(t!("Agent 最后说")).text_color(T::color(T::muted())))
                    .child(
                        div()
                            .text_size(px(T::BODY_SIZE))
                            .line_height(relative(1.6))
                            .text_color(T::color(T::body()))
                            .line_clamp(10)
                            .text_ellipsis()
                            .child(said),
                    ),
            )
            .child(actions);
        if !asks.is_empty() {
            main = main.child(section(t!("你问过"))).children(asks);
        }
        if !history.is_empty() {
            main = main.child(section(t!("经过"))).children(history);
        }
        let mut props = vec![
            prop(t!("状态"), status_icon(), status.to_owned()),
            prop(
                "Agent",
                agent_mark(s.runtime, T::color(T::secondary())),
                s.runtime.name().to_owned(),
            ),
            prop(
                t!("空间"),
                mesh(self.space_mesh(space_id), T::BOARD_MARK).into_any_element(),
                space.to_owned(),
            ),
        ];
        if let Some(branch) = self.git_branch(&s.cwd, cx) {
            props.push(prop(t!("分支"), div().into_any_element(), branch));
        }
        props.push(prop(
            t!("回合"),
            div().into_any_element(),
            content.map_or(0, |c| c.asks.len()).to_string(),
        ));
        props.push(prop(
            t!("最后动静"),
            div().into_any_element(),
            short_ago(self.session_updated(s), at),
        ));
        props.push(prop(
            t!("终端"),
            div().into_any_element(),
            if s.terminal.is_none() {
                t!("外部 · 只读").to_owned()
            } else if live {
                t!("Fibo 里在开").to_owned()
            } else {
                t!("已关掉").to_owned()
            },
        ));
        props.push(prop(t!("目录"), div().into_any_element(), s.cwd.clone()));
        let drawer = div()
            .id("board-drawer")
            .occlude()
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(relative(0.6))
            .max_w(px(T::DETAIL_WIDTH * 1.4))
            .flex()
            .bg(T::color(T::panel()))
            .border_l(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .shadow_lg()
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(main)
            .child(
                div()
                    .w(px(T::TODO_DETAIL_SIDE))
                    .flex_shrink_0()
                    .h_full()
                    .border_l(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .pt(px(T::S20 * 2.))
                    .px(px(T::S14))
                    .flex()
                    .flex_col()
                    .gap(px(T::S4))
                    .children(props),
            )
            .with_animation(
                "board-drawer-in",
                Animation::new(Duration::from_millis(450)).with_easing(spring),
                |d, t| d.right(relative(-0.6 * (1. - t))),
            );
        Some(
            div()
                .id("board-scrim")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(T::color(T::scrim()).opacity(T::BOARD_SCRIM))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.overview.drawer = None;
                    cx.notify();
                }))
                .child(drawer)
                .into_any_element(),
        )
    }
}

fn section(text: &'static str) -> gpui::Div {
    div()
        .pt(px(T::S4))
        .text_size(px(T::SMALL))
        .text_color(T::color(T::muted()))
        .child(text)
}
fn fact_row(who: &'static str, text: String, when: String) -> gpui::AnyElement {
    div()
        .flex()
        .items_start()
        .gap(px(T::S10))
        .text_size(px(T::SMALL))
        .child(
            div()
                .flex_shrink_0()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(T::color(T::body()))
                .child(who),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(T::color(T::secondary()))
                .line_clamp(2)
                .text_ellipsis()
                .child(text),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(T::color(T::muted()))
                .child(when),
        )
        .into_any_element()
}
fn prop(key: &'static str, mark: gpui::AnyElement, value: String) -> gpui::AnyElement {
    div()
        .min_h(px(T::ROW))
        .flex()
        .items_center()
        .gap(px(T::S8))
        .text_size(px(T::SMALL))
        .child(
            div()
                .w(px(T::S20 * 3.))
                .flex_shrink_0()
                .text_color(T::color(T::muted()))
                .child(key),
        )
        .child(mark)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(T::color(T::body()))
                .child(value),
        )
        .into_any_element()
}
