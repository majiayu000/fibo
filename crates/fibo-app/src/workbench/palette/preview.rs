//! The selected session's preview on the right of the palette.
use super::*;

impl Workbench {
    pub(super) fn hit_preview(
        &self,
        id: &str,
        quote: Option<&str>,
        query: &str,
        cx: &Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        use fibo_core::content::short;
        let s = self.workspace.agent_sessions.iter().find(|s| s.id == id)?;
        let (group, _) = session_status(&self.workspace, &self.attention.unseen, s);
        let content = self.workspace.content.get(&s.id);
        let place = (self.workspace.session_space(s))
            .and_then(|space| self.space_name(space))
            .unwrap_or_else(|| home_relative(&s.cwd));
        let fact = |name: &'static str, value: gpui::AnyElement| {
            div()
                .flex()
                .gap(px(T::S10))
                .text_size(px(T::SMALL))
                .child(
                    div()
                        // At least the column's width; a longer (English) name widens it
                        // rather than breaking mid-word.
                        .min_w(px(T::PALETTE_FACT))
                        .whitespace_nowrap()
                        .flex_none()
                        .text_color(T::color(T::muted()))
                        .child(name),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(T::color(T::body()))
                        .line_clamp(3)
                        .text_ellipsis()
                        .child(value),
                )
        };
        let mut facts = div().flex().flex_col().gap(px(T::S6));
        if let Some(q) = quote.filter(|_| !query.is_empty()) {
            facts = facts.child(fact(
                t!("命中"),
                highlighted(&format!("“{}”", excerpt(q, query, EXCERPT * 2)), query)
                    .into_any_element(),
            ));
        }
        let line = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        let ask = match content.map(|c| (c, &c.asks[..])) {
            Some((c, [first, .., last])) => div()
                .child(if c.first_ask_known {
                    short(&line(&first.text), 120)
                } else {
                    t!(
                        "（最初要求未覆盖）{first}",
                        first = short(&line(&first.text), 100)
                    )
                })
                .child(div().text_color(T::color(T::muted())).child(t!(
                    "追加 {n} 句，最后：{last}",
                    n = c.asks.len() - 1,
                    last = short(&line(&last.text), 40)
                ))),
            Some((_, [only])) => div().child(short(&line(&only.text), 160)),
            _ => div().child(content.map_or(t!("内容尚未索引").into(), |c| c.here().ask)),
        };
        facts = facts.child(fact(t!("你要它"), ask.into_any_element()));
        if let Some(c) = content {
            facts = facts.child(fact(
                t!("停在"),
                div().child(c.here().stopped).into_any_element(),
            ));
        }
        let resume = s.clone();
        let command = self.workspace.resume_command(s);
        Some(
            div()
                .id("search-preview")
                .w(px(T::PALETTE_PREVIEW))
                .flex_none()
                .h_full()
                .overflow_y_scroll()
                .px(px(T::S16))
                .py(px(T::S14))
                .flex()
                .flex_col()
                .gap(px(T::S10))
                .border_l(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .child(
                    div()
                        .text_size(px(T::CARD_TITLE))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(T::color(T::primary()))
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(self.session_title(s)),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(T::S6))
                        .text_size(px(T::META))
                        .text_color(T::color(T::muted()))
                        .child(chip(group))
                        .child(format!(
                            "{} · {place} · {}",
                            s.runtime.executable(),
                            ago(self.workspace.session_updated(s), now())
                        )),
                )
                .child(
                    div()
                        .text_size(px(T::META))
                        .text_color(T::color(T::secondary()))
                        .font_family(T::mono(cx).family)
                        .child(home_relative(&s.cwd)),
                )
                .child(facts)
                .when_some(content.and_then(|c| ask_track(&c.asks)), |d, track| {
                    d.child(track)
                })
                .child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(T::S6))
                        .child(
                            solid(button("search-open", ""))
                                .aria_label(t!("打开"))
                                .gap(px(T::S6))
                                .child(t!("打开"))
                                .child(kbd("↵").text_color(T::color(T::on_accent())))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.search.open = false;
                                    this.enter_card(&resume, window, cx);
                                    this.nav.pending_focus = true;
                                    cx.notify();
                                })),
                        )
                        .child(
                            quiet(button(
                                "search-copy",
                                self.copy_label("search-copy", t!("复制恢复命令")),
                            ))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.copy_text("search-copy", command.clone(), cx)
                                },
                            )),
                        ),
                ),
        )
    }
}

/// When the user spoke over the session, one tick per timed ask between the first and
/// the last; nothing for a single ask or asks without times.
fn ask_track(asks: &[fibo_core::content::Quote]) -> Option<gpui::Div> {
    let times: Vec<u64> = asks.iter().map(|q| q.at).filter(|at| *at > 0).collect();
    let (first, last) = (*times.iter().min()?, *times.iter().max()?);
    if times.len() < 2 || last <= first {
        return None;
    }
    let span = last - first;
    // The right margin keeps the last tick inside the track.
    let mut track = div().relative().mr(px(T::S2)).h(px(T::ASK_TRACK)).child(
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(T::S4))
            .h(px(T::S2))
            .rounded(px(T::BORDER_WIDTH))
            .bg(T::color(T::chip())),
    );
    for at in &times {
        let share = (at - first) as f32 / span as f32;
        track = track.child(
            div()
                .absolute()
                .top(px(T::BORDER_WIDTH))
                .left(gpui::relative(share))
                .w(px(T::S2))
                .h(px(T::ASK_TICK))
                .rounded(px(T::BORDER_WIDTH))
                .bg(T::color(T::secondary())),
        );
    }
    let hhmm = |at: u64| clock(at).chars().take(5).collect::<String>();
    Some(
        div()
            .flex()
            .flex_col()
            .gap(px(T::S4))
            .text_size(px(T::META))
            .text_color(T::color(T::muted()))
            .font_features(T::tabular())
            .child(
                div()
                    .flex()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::secondary()))
                            .child(t!("你说过 {len} 句", len = asks.len())),
                    )
                    .child(div().flex_1())
                    .child(t!("共 {span}", span = span_label(span))),
            )
            .child(track)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(hhmm(first))
                    .child(hhmm(last)),
            ),
    )
}
