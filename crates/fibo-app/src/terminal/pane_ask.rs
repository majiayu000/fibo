//! The turn's ask, pinned along the terminal's top while its row is out of sight.
use super::{ASK_FLOAT_MS, ASK_LOCATE_MS, ASK_RELOCATE_MS, Pane, PaneEvent, SCROLLBAR_HIT};
use crate::ui::theme::Theme as T;
use fibo_core::terminal::libghostty_vt::terminal::ScrollViewport;
use gpui::{Context, div, prelude::*, px};
use std::time::Instant;

impl Pane {
    /// What the conversation in this terminal was asked, oldest first. New asks are
    /// looked for on the next repaint.
    pub fn set_asks(&mut self, asks: Vec<String>, cx: &mut Context<Self>) {
        if self.ask.asks != asks {
            self.ask.asks = asks;
            self.ask.ask_rows.clear();
            self.ask.asks_located = None;
            cx.notify();
        }
    }
    /// How this terminal's CLI draws a sent ask, unless it pins the turn's ask itself.
    pub(super) fn prompt_line(&self) -> Option<&'static fibo_core::agent::PromptLine> {
        let prompt = &self.agent?.spec()?.prompt;
        (!prompt.pins_own).then_some(prompt)
    }
    /// The newest ask while its row is above the viewport, and whether it can be
    /// scrolled back to.
    pub fn pinned_latest(&self) -> Option<(String, bool)> {
        let (i, row) = self.ask.pinned.latest?;
        Some((self.ask.asks.get(i)?.clone(), row.is_some()))
    }
    /// Shows the newest ask's row at the top.
    pub fn back_to_latest_ask(&mut self, cx: &mut Context<Self>) {
        if let Some((_, Some(row))) = self.ask.pinned.latest {
            self.back_to_ask(row, cx);
        }
    }
    fn back_to_ask(&mut self, row: usize, cx: &mut Context<Self>) {
        self.scroll_to(ScrollViewport::Row(row), cx);
    }
    /// Works out what to pin from where the asks were last found, and looks for them
    /// again once output may have moved them. Runs on each repaint.
    pub(super) fn refresh_pinned(&mut self, cx: &mut Context<Self>) {
        let choice = crate::ui::pinned_ask::current(cx);
        let was = self.ask.pinned.latest.is_some();
        let prompt = self.prompt_line().filter(|_| choice.any());
        match prompt {
            Some(prompt) if !self.ask.asks.is_empty() => {
                self.ask.pinned = if self.ask.ask_rows.len() == self.ask.asks.len() {
                    fibo_core::prompt_pin::pinned(&self.ask.ask_rows, self.viewport)
                } else {
                    Default::default()
                };
                let due = match self.ask.asks_located {
                    None => true,
                    Some((at, total)) => {
                        let ms = at.elapsed().as_millis() as u64;
                        (total != self.viewport.1 && ms >= ASK_LOCATE_MS) || ms >= ASK_RELOCATE_MS
                    }
                };
                if due && !self.ask.locating_asks {
                    self.locate_asks(prompt, cx);
                }
            }
            _ => self.ask.pinned = Default::default(),
        }
        if was != self.ask.pinned.latest.is_some() {
            cx.emit(PaneEvent::PinnedAsk);
        }
    }
    fn locate_asks(
        &mut self,
        prompt: &'static fibo_core::agent::PromptLine,
        cx: &mut Context<Self>,
    ) {
        self.ask.locating_asks = true;
        let asks = self.ask.asks.clone();
        let keys: Vec<_> = asks.iter().map(|a| fibo_core::prompt_pin::key(a)).collect();
        let state = self.connection.state.clone();
        let job = cx.background_spawn(async move {
            let state = state.lock().ok()?;
            let vt = state.vt.as_ref()?;
            let total = vt.viewport().ok()?.1;
            Some((vt.find_prompts(&keys, prompt).ok()?, total))
        });
        cx.spawn(async move |this, cx| {
            let found = job.await;
            let _ = this.update(cx, |this, cx| {
                this.ask.locating_asks = false;
                // The asks changed while looking: the next repaint looks again.
                if this.ask.asks != asks {
                    return;
                }
                let Some((rows, total)) = found else { return };
                this.ask.asks_located = Some((Instant::now(), total));
                if this.ask.ask_rows != rows {
                    this.ask.ask_rows = rows;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// The pinned ask along the terminal's top, and the chip naming the ask whose answer
    /// is being read in history; each as the settings choose.
    pub(super) fn ask_overlays(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let choice = crate::ui::pinned_ask::current(cx);
        let first_line = |i: usize| self.ask_first_line(i);
        let mut out = Vec::new();
        let bar = self.ask_bar_line(cx);
        let bar_shown = bar.is_some();
        // Exactly the terminal's first row, marked as the CLI marks a sent ask.
        let row_height = self
            .metrics
            .map_or(px(T::MONO_SIZE * T::LINE_HEIGHT), |m| m.height);
        if let Some((text, row)) = bar {
            let mark = self.ask_mark();
            out.push(
                div()
                    .id("pinned-ask")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right(px(SCROLLBAR_HIT))
                    .h(row_height)
                    .pr(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .bg(T::color(T::button()))
                    .font(T::mono(cx))
                    .text_size(px(T::MONO_SIZE))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(T::color(T::muted()))
                            .child(mark),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(T::color(T::primary()))
                            .child(text),
                    )
                    .when_some(row, |d, row| {
                        d.cursor_pointer()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(T::META))
                                    .text_color(T::color(T::muted()))
                                    .child(fibo_core::i18n::text(super::super::glyph::ASK_BACK)),
                            )
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.back_to_ask(row, cx);
                                window.focus(&this.focus, cx);
                            }))
                    })
                    .into_any_element(),
            );
        }
        let reading = self.scrolled_back
            && self
                .ask
                .scrolled_at
                .is_some_and(|at| at.elapsed().as_millis() < u128::from(ASK_FLOAT_MS));
        let float = self
            .ask
            .pinned
            .viewed
            .or(self
                .ask
                .pinned
                .latest
                .filter(|_| !bar_shown)
                .and_then(|(i, row)| Some((i, row?))))
            .filter(|_| choice.float && reading)
            .and_then(|(i, row)| Some((first_line(i)?, row)));
        if let Some((text, row)) = float {
            out.push(
                div()
                    .absolute()
                    .top(if bar_shown {
                        row_height + px(T::S6)
                    } else {
                        px(T::S8)
                    })
                    .left_0()
                    .right(px(SCROLLBAR_HIT))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id("viewed-ask")
                            .max_w(gpui::relative(0.7))
                            .px(px(T::S12))
                            .py(px(T::S2))
                            .rounded(px(T::S12))
                            .bg(T::color(T::raised()))
                            .border(px(T::BORDER_WIDTH))
                            .border_color(T::color(T::card_border()))
                            .shadow_sm()
                            .text_size(px(T::META))
                            .text_color(T::color(T::secondary()))
                            .truncate()
                            .cursor_pointer()
                            .child(t!("在看 › {text}", text))
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_click(cx.listener(move |this, _, _, cx| this.back_to_ask(row, cx))),
                    )
                    .into_any_element(),
            );
        }
        out
    }
}
