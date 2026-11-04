//! 插件's two tabs at their top, and 发现: what can be had, by what it is for, as cards you
//! can point at to watch rather than rows to read.
use super::catalog::{Item, playing};
use super::showcase::ThemeLook;
use super::*;
use crate::workbench::complete::{Play, stage};
use mooring_core::plugin_category::Category;
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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::workbench) enum Tab {
    Installed,
    #[default]
    Discover,
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
                        format!("已安装 {installed}"),
                        Tab::Installed,
                        cx,
                    ))
                    .child(choose(
                        "plugin-tab-discover",
                        format!("发现 {total}"),
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
        let id = &item.id;
        let quiet = |text: String, on: bool| {
            div()
                .flex_shrink_0()
                .text_size(px(T::SMALL))
                .text_color(T::color(if on { T::primary() } else { T::secondary() }))
                .child(text)
                .into_any_element()
        };
        if self.market.working.as_deref() == Some(id) {
            return quiet("正在处理…".into(), false);
        }
        let busy = self.market.working.is_some();
        if let Some(offer) = item.offer.clone() {
            let text = if item.update {
                format!("更新到 {}", offer.entry.version)
            } else if item.complete.is_some() {
                "安装并使用".to_owned()
            } else {
                "安装".to_owned()
            };
            let key = format!("plugin-get-{id}");
            let get = if strong && !item.update {
                filled(key, text)
            } else {
                secondary(key, text)
            };
            return get
                .when(busy, |b| b.opacity(0.5))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if this.market.working.is_none() {
                        this.install_offer(offer.clone(), cx);
                    }
                }))
                .into_any_element();
        }
        if let Some(pick) = item.pick.clone() {
            if pick == playing() {
                return quiet("● 正在用".into(), true);
            }
            return secondary(
                format!("complete-use-{}", pick.name().replace(':', "-")),
                "用这个",
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.text_error = mooring_core::effect::pick(Some(pick.clone())).err();
                cx.notify();
            }))
            .into_any_element();
        }
        if item.complete.is_some() && !item.enabled() {
            let on = id.clone();
            return secondary(format!("complete-enable-{id}"), "打开并用这个")
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if this.market.working.is_none() {
                        this.switch_plugin(on.clone(), true, false, cx);
                    }
                }))
                .into_any_element();
        }
        quiet(
            if item.enabled() {
                "● 已安装"
            } else {
                "已停用"
            }
            .to_owned(),
            item.enabled(),
        )
    }
    /// A stage that plays while pointed at, and the item's name for the click to open.
    pub(super) fn item_stage(&self, item: &Item, complete: Complete) -> impl IntoElement {
        let play = match &self.market.hover {
            Some((id, from)) if *id == item.id => Play::Loop(*from),
            _ => Play::Still,
        };
        stage(complete, play)
    }
    fn item_card(&self, item: &Item, looks: &[ThemeLook], cx: &Context<Self>) -> gpui::AnyElement {
        let id = item.id.clone();
        let hover_id = id.clone();
        let on = item.pick.as_ref().is_some_and(|p| *p == playing());
        div()
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
                    cx.notify();
                } else if !*hovered && was {
                    this.market.hover = None;
                    cx.notify();
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
                            .child(self.item_control(item, false, cx)),
                    )
                    .child(
                        div()
                            .h(px(CARD_NOTE))
                            .overflow_hidden()
                            .child(label(item.description.clone().unwrap_or_default())),
                    ),
            )
            .into_any_element()
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
            .child(self.plugin_chip("plugin-cat-all".into(), format!("全部 {total}"), None, cx));
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
            let more = (capped && rows.len() > shown.len()).then(|| {
                secondary(
                    "plugin-see-animations",
                    format!("查看全部 {} 个", rows.len()),
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
                        .grid_cols(if animated {
                            COLUMNS_ANIMATION
                        } else {
                            COLUMNS_TEXT
                        })
                        .gap(px(T::S12))
                        .children(shown.iter().map(|i| {
                            let looks = self
                                .market
                                .showcase
                                .get(&i.id)
                                .map_or(&empty, |s| &s.themes);
                            self.item_card(i, looks, cx)
                        }))
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
                    .child(label(format!(
                        "没有找到「{}」",
                        self.market.search_text(cx)
                    ))),
            );
        }
        page.child(self.remote_row(cx))
    }
}
