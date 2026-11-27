//! Open a saved message in place, with the search and its page still available on Back.
use super::*;

pub(super) struct OpenedMessage {
    hit: MessageMatch,
    text: Option<Result<gpui::SharedString, ()>>,
}

impl Workbench {
    pub(super) fn open_search_message(&mut self, hit: MessageMatch, cx: &mut Context<Self>) {
        // Stay on this result's page on Back even if the next page was in flight.
        self.search.message_search = None;
        let position = hit.position.clone();
        let generation = hit.generation.clone();
        let read = cx.background_spawn(self.store.send(Request::ReadSearchMessage {
            position: position.clone(),
            generation: generation.clone(),
        }));
        self.search.opened_message = Some(OpenedMessage { hit, text: None });
        self.pane_changed(cx);
        cx.spawn(async move |this, cx| {
            let reply = read.await;
            let _ = this.update(cx, |this, cx| {
                let Some(open) = &mut this.search.opened_message else {
                    return;
                };
                if open.hit.position != position || open.hit.generation != generation {
                    return;
                }
                open.text = Some(match reply {
                    Ok(Reply::Text(text)) => Ok(gpui::SharedString::from(text)),
                    _ => Err(()),
                });
                this.pane_changed(cx);
            });
        })
        .detach();
    }

    pub(super) fn back_to_search_results(&mut self, cx: &mut Context<Self>) -> bool {
        if self.search.opened_message.take().is_none() {
            return false;
        }
        self.search.scroll.scroll_to_item(self.search.index);
        self.pane_changed(cx);
        true
    }

    pub(super) fn search_message_body(
        &self,
        open: &OpenedMessage,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let hit = &open.hit;
        let id = hit.position.session.clone();
        let title = self
            .workspace
            .agent_sessions
            .iter()
            .find(|s| s.id == id)
            .map(|s| self.session_title(s))
            .unwrap_or_default();
        let source = if hit.user {
            t!("你")
        } else {
            t!("Agent 回复")
        };
        let bar = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(T::S8))
            .child(
                quiet(button("search-message-back", t!("返回搜索结果"))).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.back_to_search_results(cx);
                    },
                )),
            )
            .child(div().min_w_0().flex_1().truncate().child(title))
            .when_some(
                open.text.as_ref().and_then(|text| text.as_ref().ok()),
                |d, text| {
                    let text = text.clone();
                    d.child(
                        quiet(button(
                            "search-message-copy",
                            self.copy_label("search-message-copy", t!("复制原消息")),
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy_text("search-message-copy", text.to_string(), cx);
                        })),
                    )
                },
            )
            .child(
                quiet(button("search-message-session", t!("打开会话"))).on_click(cx.listener(
                    move |this, _, window, cx| {
                        this.open_hit(
                            Hit::Session {
                                id: id.clone(),
                                quote: None,
                            },
                            window,
                            cx,
                        );
                    },
                )),
            );
        let text = match &open.text {
            None => label(t!("正在读取原消息…")).into_any_element(),
            Some(Err(())) => label(t!("无法读取这条消息，请重新搜索")).into_any_element(),
            Some(Ok(text)) => div()
                .whitespace_normal()
                .child(text.clone())
                .into_any_element(),
        };
        div()
            .h(px(T::PALETTE_LIST))
            .flex()
            .flex_col()
            .gap(px(T::S8))
            .px(px(T::S16))
            .py(px(T::S10))
            .child(bar)
            .child(
                div()
                    .flex_none()
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::muted()))
                    .child(format!(
                        "{source} · {}",
                        ago(hit.position.at.max(0) as u64, now())
                    )),
            )
            .child(
                div()
                    .id("search-message-text")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .text_size(px(T::SMALL))
                    .child(text),
            )
    }

    pub(super) fn message_page_buttons(
        &self,
        query: &str,
        cx: &Context<Self>,
    ) -> Option<gpui::Div> {
        let page = self
            .search
            .message_result
            .as_ref()
            .filter(|page| page.query == query)?;
        if page.trail.is_empty() && page.next.is_none() {
            return None;
        }
        let can_turn = self.search.message_search.is_none();
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(T::S6))
            .child(t!("第 {page} 页", page = page.trail.len() + 1));
        if !page.trail.is_empty() {
            row = row.child(
                quiet(button("search-messages-newer", t!("更新的消息"))).when(can_turn, |b| {
                    b.on_click(cx.listener(|this, _, _, cx| this.turn_message_page(false, cx)))
                }),
            );
        }
        if page.next.is_some() {
            row = row.child(
                quiet(button("search-messages-older", t!("更早的消息"))).when(can_turn, |b| {
                    b.on_click(cx.listener(|this, _, _, cx| this.turn_message_page(true, cx)))
                }),
            );
        }
        Some(row)
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    pub(crate) fn visual_message_page(&self) -> Option<(usize, usize)> {
        self.search
            .message_result
            .as_ref()
            .map(|page| (page.trail.len() + 1, page.hits.len()))
    }

    pub(crate) fn visual_opened_search_message(&self) -> Option<String> {
        self.search
            .opened_message
            .as_ref()?
            .text
            .as_ref()?
            .as_ref()
            .ok()
            .map(ToString::to_string)
    }
}
