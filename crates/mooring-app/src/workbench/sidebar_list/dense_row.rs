use super::*;

impl Workbench {
    /// Questions waiting in a row: how many the agent asked at once, at least one.
    fn asked(&self, s: &AgentSession) -> usize {
        self.workspace
            .content
            .get(&s.id)
            .map_or(1, |c| {
                c.waiting.lines().filter(|l| !l.trim().is_empty()).count()
            })
            .max(1)
    }
    /// A row in the 状态 / 目录 presets (and plugin presets beside them), after
    /// sidebar-density.html: the CLI's mark, the title in the system font, then the
    /// running turn's clock or a short age (`4m`, `2h`) and the state symbol, both
    /// monospace. The second line comes for what needs you, what runs, and the selected row.
    pub(super) fn dense_row(
        &self,
        (session, g, attention): Listed,
        current: Option<&str>,
        detail: Option<String>,
        end: End,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let open = session.clone();
        use crate::ui::status_colors as seen;
        let selected = current == Some(session.id.as_str());
        let mono = T::mono(cx).family;
        // Every row and tab shows its CLI's mark, in its own color (v3 mock).
        let marks = true;
        let title = match self
            .layout
            .renaming_session
            .as_ref()
            .filter(|(id, ..)| id == &session.id)
        {
            Some((_, field, _)) => div().flex_1().min_w_0().child(field.clone()),
            None => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(T::UI_FONT)
                .text_size(px(T::SMALL))
                .text_color(if attention || selected {
                    T::color(T::primary())
                } else {
                    T::color(seen::seen_title())
                })
                .child(self.session_title(session)),
        };
        let time = match end {
            End::Text(text) => text,
            End::Nothing => String::new(),
            // Only a running turn ticks; a stopped, waking or closed row shows its age.
            End::State => match self.turn_started(session).filter(|_| g == Status::Running) {
                Some(at) => {
                    self.tick_turn(cx);
                    super::super::layout::turn_clock(now().saturating_sub(at))
                }
                None => short_ago(self.session_updated(session), now()),
            },
        };
        // A finished turn not seen yet is a filled green dot; once seen it keeps
        // 等你验收's ring and its green.
        let state = || {
            div()
                .w(px(T::S12))
                .flex_shrink_0()
                .flex()
                .justify_center()
                .font_family(mono.clone())
                .font_weight(gpui::FontWeight::BOLD)
                .text_size(px(T::SMALL))
                .child(self.session_mark(g, attention, true, cx))
        };
        div()
            .id(gpui::SharedString::from(format!("session-{}", session.id)))
            .px(px(T::S8))
            .py(px(T::S4))
            .flex()
            .flex_col()
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .when(selected, |d| d.bg(T::color(T::raised())))
            .hover(|s| s.bg(T::color(T::raised())))
            .when(self.peek_home(&session.id), |d| {
                d.relative().child(super::super::peek::peek_home_outline())
            })
            .when(self.marking(&session.id), |d| {
                d.relative().child(super::super::complete::spot(
                    &self.layout.spots,
                    session.id.clone(),
                    (T::S8, T::S4),
                    |s| &mut s.rows,
                ))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .when(marks, |d| {
                        d.child(super::super::board::agent_mark(
                            session.runtime,
                            T::color(T::muted()),
                        ))
                    })
                    .child(title)
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(mono.clone())
                            .font_features(T::tabular())
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted()))
                            .child(time),
                    )
                    .child(state()),
            )
            .when_some(detail, |d, text| {
                // A second line that opens with the state word shows that word in its ink;
                // a question is in 等你回答's ink throughout, with how many it asks.
                let asks = text.starts_with(mooring_core::i18n::say(ASKS));
                let (state, rest) = match text.strip_prefix(g.label()) {
                    Some(rest) => (Some(g.label()), rest.to_owned()),
                    None => (None, text),
                };
                // What the turn changed leads, as the v3 rows have it: +128 −14.
                let lines = self
                    .workspace
                    .content
                    .get(&session.id)
                    .map(|c| c.turn_lines())
                    .filter(|(a, r)| a + r > 0 && !asks);
                d.child(
                    div()
                        .when(marks, |d| d.pl(px(T::BOARD_MARK + T::S8)))
                        .flex()
                        .items_center()
                        .gap(px(T::S4))
                        .min_w_0()
                        .font_family(T::UI_FONT)
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(if asks {
                            g.colors().1
                        } else {
                            T::color(T::secondary())
                        })
                        .when_some(lines, |d, lines| {
                            d.child(super::super::layout::line_counts(lines))
                                .child(div().text_color(T::color(T::muted())).child("·"))
                        })
                        .when_some(state, |d, word| {
                            d.child(div().flex_shrink_0().text_color(g.colors().1).child(word))
                        })
                        .child(div().flex_1().min_w_0().truncate().child(rest))
                        .when(asks && attention, |d| {
                            d.child(
                                div()
                                    .flex_shrink_0()
                                    .size(px(T::S14))
                                    .rounded_full()
                                    .bg(g.colors().0)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(T::color(T::terminal_bg()))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(self.asked(session).to_string()),
                            )
                        }),
                )
            })
            // The hover card would cover the right-click menu.
            .when(self.layout.session_menu.is_none(), |d| {
                d.tooltip(super::super::diagnostics::tip_lines(
                    self.session_tip(session),
                ))
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                self.open_session_menu(session, cx),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.click_count() == 2 {
                        this.rename_session(&open.id, window, cx);
                        return;
                    }
                    this.peek_or_open(&open, window, cx)
                }),
            )
            .into_any_element()
    }
}
