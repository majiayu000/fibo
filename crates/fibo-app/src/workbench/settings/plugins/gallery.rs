//! 插件's two tabs at their top, and 发现: what can be had, by what it is for, as cards you
//! can point at to watch rather than rows to read.
use super::catalog::{Item, playing};
use super::showcase::ThemeLook;
use super::*;
use crate::workbench::complete::stage;
use fibo_core::plugin_category::Category;
use std::time::Instant;

/// Tall enough for the made-up terminal and a face over it.
const CARD_STAGE: f32 = 118.;
const THEME_STRIP: f32 = 92.;
/// Lines of description a card keeps, so cards in a row end together.
const CARD_NOTE: f32 = 34.;
const CHIP_HEIGHT: f32 = 26.;
const SEARCH_WIDTH: f32 = 200.;
/// How many animations 全部 shows before 查看全部: one row, so the other kinds stay in sight.
const ANIMATIONS_IN_ALL: usize = 3;
const COLUMNS_ANIMATION: u16 = 3;
const COLUMNS_TEXT: u16 = 2;

/// A card's height, from the parts it stacks: the grid sizing its rows by laying every
/// card out to learn it was half of laying out the page (R9). The name row is as tall as
/// its button, or as one line of the name without one.
fn card_height(item: &Item, looks: &[ThemeLook], button: bool) -> f32 {
    let stage = if item.complete.is_some() {
        CARD_STAGE
    } else {
        0.
    };
    let strip = if looks.is_empty() { 0. } else { THEME_STRIP };
    let name = if button {
        SEGMENT_HEIGHT
    } else {
        T::BODY_SIZE * T::LINE_HEIGHT
    };
    let words = T::S10 * 2. + name + T::S4 + CARD_NOTE;
    T::BORDER_WIDTH * 2. + stage + strip + words
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::workbench) enum Tab {
    Installed,
    #[default]
    Discover,
}

pub(super) fn switch_notice() -> Option<gpui::Div> {
    fibo_core::plugin::switch_error().map(|why| {
        div()
            .px(px(T::S16))
            .py(px(T::S10))
            .text_size(px(T::SMALL))
            .text_color(Tone::Danger.ink())
            .child(t!("无法读取插件开关。重新加载插件时将保持停用。{why}", why))
    })
}

pub(super) fn mark(c: Category) -> &'static str {
    match c {
        Category::Effects => "✓",
        Category::Look => "◐",
        Category::Sidebar => "▤",
        Category::Status => "◉",
        Category::Commands => ">_",
        Category::Agents => "◇",
    }
}

/// Text in the terminal's font showing how a theme's colors read together.
pub(super) fn theme_sample(look: &ThemeLook) -> gpui::Stateful<gpui::Div> {
    let line = |text: &str, color: u32| {
        div()
            .text_size(px(T::SMALL))
            .text_color(T::color(color))
            .child(text.to_owned())
    };
    let image = look.image.clone();
    let strength = look.material.image.as_ref().map_or(0, |i| i.strength);
    div()
        .id(gpui::SharedString::from(format!("theme-{}", look.name)))
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        // What shows through a see-through window: a desk of some color, not nothing.
        .bg(T::color(look.blue))
        .child(
            div()
                .absolute()
                .inset_0()
                .bg(T::color(look.bg).opacity(f32::from(look.material.opacity) / 100.)),
        )
        .when_some(image, |d, path| {
            d.child(
                gpui::img(path)
                    .absolute()
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover)
                    .opacity(f32::from(strength) / 100.),
            )
        })
        .child(
            div()
                .relative()
                .p(px(T::S8))
                .flex()
                .flex_col()
                .child(line(&look.name, look.fg))
                .child(line("✓ test ok", look.green))
                .child(line("✗ 1 failed", look.red))
                .child(line("~/mooring", look.blue)),
        )
}

/// The one chosen or most wanted control: ink on the page's color, staying readable when
/// pointed at, which a plain button's hover color would not.
fn filled(
    id: impl Into<gpui::ElementId>,
    text: impl Into<gpui::SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let text = text.into();
    div()
        .id(id)
        .role(gpui::accesskit::Role::Button)
        .aria_label(text.clone())
        .flex_shrink_0()
        .h(px(SEGMENT_HEIGHT))
        .px(px(T::S10))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(T::CONTROL_RADIUS))
        .cursor_pointer()
        .text_size(px(T::SMALL))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::primary()))
        .bg(T::color(T::primary()))
        .text_color(T::color(T::popover()))
        .child(text)
}

impl SettingsView {
    pub(super) fn plugin_tabs(
        &self,
        installed: usize,
        total: usize,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let tab = self.market.tab;
        let choose = |id: &'static str, text: String, which: Tab, cx: &Context<Self>| {
            segment(id, text, tab == which).on_click(cx.listener(move |this, _, _, cx| {
                this.market.tab = which;
                this.market.open = None;
                this.sync_player(cx);
                this.market.scroll.set_offset(gpui::Point::default());
                cx.notify();
            }))
        };
        div()
            .pt(px(T::S8))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(T::S12))
            .child(
                segments()
                    .child(choose(
                        "plugin-tab-installed",
                        t!("已安装 {installed}", installed),
                        Tab::Installed,
                        cx,
                    ))
                    .child(choose(
                        "plugin-tab-discover",
                        t!("发现 {total}", total),
                        Tab::Discover,
                        cx,
                    )),
            )
            .when(tab == Tab::Discover, |d| {
                d.child(
                    div()
                        .w(px(SEARCH_WIDTH))
                        .children(self.market.search_field(cx)),
                )
            })
    }
    fn plugin_chip(
        &self,
        id: String,
        text: String,
        which: Option<Category>,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let searching = !self.market.query(cx).is_empty();
        let on = if searching {
            which.is_none()
        } else {
            self.market.category == which
        };
        let chip = if on {
            filled(id, text)
        } else {
            secondary(id, text)
        };
        chip.h(px(CHIP_HEIGHT))
            .rounded_full()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.market.category = which;
                this.market.scroll.set_offset(gpui::Point::default());
                cx.notify();
            }))
    }
    /// What the card, row and detail view all offer: the one next step for this plugin.
    pub(super) fn item_control(
        &self,
        item: &Item,
        strong: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        self.next_step(item, strong, cx).0
    }
    /// `item_control`, and whether it is a button (as tall as `SEGMENT_HEIGHT`) rather than
    /// a line of text.
    fn next_step(&self, item: &Item, strong: bool, cx: &Context<Self>) -> (gpui::AnyElement, bool) {
        let id = &item.id;
        let quiet = |text: String, on: bool| {
            let text = div()
                .flex_shrink_0()
                .text_size(px(T::SMALL))
                .text_color(T::color(if on { T::primary() } else { T::secondary() }))
                .child(text)
                .into_any_element();
            (text, false)
        };
        if self.market.working.as_deref() == Some(id) {
            return quiet(t!("正在处理…").into(), false);
        }
        let busy = self.market.working.is_some();
        if let Some(offer) = item.offer.clone() {
            let text = if item.update {
                t!("更新到 {version}", version = offer.entry.version)
            } else if item.complete.is_some() {
                t!("安装并使用").to_owned()
            } else {
                t!("安装").to_owned()
            };
            let key = format!("plugin-get-{id}");
            let get = if strong && !item.update {
                filled(key, text)
            } else {
                secondary(key, text)
            };
            let get =
                get.when(busy, |b| b.opacity(0.5))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if this.market.working.is_none() {
                            this.install_offer(offer.clone(), cx);
                        }
                    }));
            return (get.into_any_element(), true);
        }
        if let Some(pick) = item.pick.clone() {
            if pick == playing() {
                return quiet(t!("● 正在用").into(), true);
            }
            let use_it = secondary(
                format!("complete-use-{}", pick.name().replace(':', "-")),
                t!("用这个"),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.text_error = fibo_core::effect::pick(Some(pick.clone())).err();
                cx.notify();
            }));
            return (use_it.into_any_element(), true);
        }
        if item.complete.is_some() && !item.enabled() {
            let on = id.clone();
            let enable = secondary(format!("complete-enable-{id}"), t!("打开并用这个")).on_click(
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if this.market.working.is_none() {
                        this.switch_plugin(on.clone(), true, false, cx);
                    }
                }),
            );
            return (enable.into_any_element(), true);
        }
        quiet(
            if item.enabled() {
                t!("● 已安装")
            } else {
                t!("已停用")
            }
            .to_owned(),
            item.enabled(),
        )
    }
    /// A stage that plays while pointed at, and the item's name for the click to open.
    pub(super) fn item_stage(&self, item: &Item, complete: Complete) -> impl IntoElement {
        let spot =
            (self.market.play_id() == Some(item.id.as_str())).then(|| self.market.spot.clone());
        stage(complete, spot)
    }
    /// A card without its height, and the height its parts add up to: a grid row is as tall
    /// as its tallest card, and every card in it stretches to that.
    fn item_card(
        &self,
        item: &Item,
        looks: &[ThemeLook],
        cx: &Context<Self>,
    ) -> (gpui::Stateful<gpui::Div>, f32) {
        let id = item.id.clone();
        let hover_id = id.clone();
        let on = item.pick.as_ref().is_some_and(|p| *p == playing());
        let (control, button) = self.next_step(item, false, cx);
        let card = div()
            .id(gpui::SharedString::from(format!("plugin-card-{id}")))
            .role(gpui::accesskit::Role::Button)
            .aria_label(gpui::SharedString::from(item.name.clone()))
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .cursor_pointer()
            .rounded(px(T::CONTROL_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(if on {
                Tone::Ok.dot()
            } else {
                T::color(T::border())
            })
            .bg(T::color(T::popover()))
            .hover(|s| s.border_color(T::color(T::control())))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let was = this
                    .market
                    .hover
                    .as_ref()
                    .is_some_and(|(i, _)| *i == hover_id);
                if *hovered && !was {
                    this.market.hover = Some((hover_id.clone(), Instant::now()));
                    this.sync_player(cx);
                } else if !*hovered && was {
                    this.market.hover = None;
                    this.sync_player(cx);
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.open_item(id.clone(), cx)))
            .when_some(item.complete, |d, complete| {
                d.child(
                    div()
                        .w_full()
                        .h(px(CARD_STAGE))
                        .child(self.item_stage(item, complete)),
                )
            })
            .when(!looks.is_empty(), |d| {
                d.child(
                    div()
                        .w_full()
                        .h(px(THEME_STRIP))
                        .flex()
                        .children(looks.iter().map(theme_sample)),
                )
            })
            .child(
                div()
                    .p(px(T::S10))
                    .flex()
                    .flex_col()
                    .gap(px(T::S4))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(T::S8))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(T::BODY_SIZE))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(T::color(T::primary()))
                                    .child(item.name.clone()),
                            )
                            .child(control),
                    )
                    .child(
                        div()
                            .h(px(CARD_NOTE))
                            .overflow_hidden()
                            .child(label(item.description.clone().unwrap_or_default())),
                    ),
            );
        (card, card_height(item, looks, button))
    }
    pub(super) fn discover_tab(&self, items: &[Item], cx: &Context<Self>) -> gpui::Div {
        let listed = |c: Category| {
            items
                .iter()
                .filter(|i| i.category == c && !i.builtin())
                .count()
        };
        let total = items.iter().filter(|i| !i.builtin()).count();
        let query = self.market.query(cx);
        let mut chips = div()
            .pt(px(T::S12))
            .flex()
            .flex_wrap()
            .gap(px(T::S6))
            .child(self.plugin_chip("plugin-cat-all".into(), t!("全部 {total}", total), None, cx));
        for c in Category::ALL {
            chips = chips.child(self.plugin_chip(
                format!("plugin-cat-{}", c.label()),
                format!("{} {}", c.label(), listed(c)),
                Some(c),
                cx,
            ));
        }
        let mut page = div().flex().flex_col().child(chips);
        // An install started from a card has only this place to say it failed.
        if let Some(said) = &self.market.done {
            let (ink, text) = match said {
                Ok(t) => (T::color(T::secondary()), t.clone()),
                Err(e) => (Tone::Danger.ink(), e.clone()),
            };
            page = page.child(
                div()
                    .pt(px(T::S10))
                    .text_size(px(T::SMALL))
                    .text_color(ink)
                    .child(text),
            );
        }
        let mut found = false;
        for c in Category::ALL {
            // A search looks through everything: the chip chosen before it is no reason to
            // say nothing was found.
            if query.is_empty() && self.market.category.is_some_and(|only| only != c) {
                continue;
            }
            let rows: Vec<&Item> = items
                .iter()
                .filter(|i| i.category == c && !i.builtin() && i.answers(&query))
                .collect();
            if rows.is_empty() {
                continue;
            }
            found = true;
            let capped =
                c == Category::Effects && self.market.category.is_none() && query.is_empty();
            let shown = if capped {
                &rows[..rows.len().min(ANIMATIONS_IN_ALL)]
            } else {
                &rows[..]
            };
            let animated = shown.iter().any(|i| i.complete.is_some());
            let columns = if animated {
                COLUMNS_ANIMATION
            } else {
                COLUMNS_TEXT
            };
            let more = (capped && rows.len() > shown.len()).then(|| {
                secondary(
                    "plugin-see-animations",
                    t!("查看全部 {len} 个", len = rows.len()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.market.category = Some(c);
                    this.market.scroll.set_offset(gpui::Point::default());
                    cx.notify();
                }))
            });
            let empty = Vec::new();
            page = page
                .child(
                    div()
                        .flex()
                        .items_end()
                        .justify_between()
                        .pb(px(T::S4))
                        .child(heading(c.label()))
                        .children(more),
                )
                .child(
                    div()
                        .grid()
                        .grid_cols(columns)
                        .gap(px(T::S12))
                        .children(rows_of_cards(
                            shown.iter().map(|i| {
                                let looks = self
                                    .market
                                    .showcase
                                    .get(&i.id)
                                    .map_or(&empty, |s| &s.themes);
                                self.item_card(i, looks, cx)
                            }),
                            columns,
                        ))
                        .when(
                            c == Category::Effects
                                && self.market.category == Some(c)
                                && query.is_empty(),
                            |d| d.child(self.face_tile(cx)),
                        ),
                );
        }
        if !found {
            page = page.child(
                div()
                    .py(px(T::S20 * 2.))
                    .flex()
                    .justify_center()
                    .child(label(t!(
                        "没有找到「{query}」",
                        query = self.market.search_text(cx)
                    ))),
            );
        }
        page.child(self.remote_row(cx))
    }
}

/// Cards in a grid of `columns`, each as tall as the tallest in its row.
fn rows_of_cards(
    cards: impl Iterator<Item = (gpui::Stateful<gpui::Div>, f32)>,
    columns: u16,
) -> Vec<gpui::Stateful<gpui::Div>> {
    let cards: Vec<_> = cards.collect();
    let mut out = Vec::with_capacity(cards.len());
    let mut cards = cards.into_iter().peekable();
    while cards.peek().is_some() {
        let row: Vec<_> = cards.by_ref().take(columns as usize).collect();
        let tallest = row.iter().map(|(_, h)| *h).fold(0., f32::max);
        out.extend(row.into_iter().map(|(card, _)| card.h(px(tallest))));
    }
    out
}
