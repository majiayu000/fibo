//! 会话优先's space chips, which narrow the list below them to one space.
use super::*;

/// 「新空间」, or 「新空间 2」 and on when that name is taken.
pub(super) fn new_space_name(spaces: &[Space]) -> String {
    (1..)
        .map(|i| match i {
            1 => "新空间".to_owned(),
            i => format!("新空间 {i}"),
        })
        .find(|name| !spaces.iter().any(|s| &s.name == name))
        .unwrap_or_default()
}
impl Workbench {
    pub(super) fn first_agent(&self) -> Agent {
        self.tasks
            .last_agent
            .unwrap_or(self.workspace.settings.default_agent)
    }
    /// The window wears the current space's theme; asked on every render, and cheap.
    pub(super) fn follow_space_theme(&self, cx: &mut Context<Self>) {
        use crate::ui::space_themes;
        if space_themes::needs_follow(&self.nav.space, cx) {
            let space = self.nav.space.clone();
            cx.defer(move |cx| space_themes::follow(&space, cx));
        }
    }
    /// The space menu's 主题 row and, unfolded, every theme to give this space.
    pub(super) fn space_theme_rows(&self, id: &str, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        use crate::ui::{
            space_themes::{self, SpaceThemes},
            themes::Themes,
        };
        let Some(spaces) = cx.try_global::<SpaceThemes>() else {
            return vec![];
        };
        let Some(themes) = cx.try_global::<Themes>() else {
            return vec![];
        };
        let on = spaces.of(id);
        let named = on.and_then(|on| themes.get(on)).map(|e| e.name.as_str());
        let mut rows = vec![
            button(
                "space-menu-theme",
                format!(
                    "主题：{} {}",
                    named.unwrap_or("跟随外观"),
                    if spaces.menu_open { "▾" } else { "▸" }
                ),
            )
            .w_full()
            .justify_start()
            .border_0()
            .on_click(cx.listener(|_, _, _, cx| {
                space_themes::toggle_menu(cx);
                cx.notify();
            }))
            .into_any_element(),
        ];
        if !spaces.menu_open {
            return rows;
        }
        let choices = std::iter::once((None, "跟随外观".to_owned())).chain(
            themes
                .entries
                .iter()
                .map(|e| (Some(e.id.clone()), e.name.clone())),
        );
        for (i, (theme, name)) in choices.enumerate() {
            let picked = theme.as_deref() == on;
            let space = id.to_owned();
            rows.push(
                button(
                    gpui::SharedString::from(format!("space-theme-{i}")),
                    format!("{} {name}", if picked { "✓" } else { "　" }),
                )
                .w_full()
                .justify_start()
                .border_0()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.layout.space_menu = None;
                    space_themes::toggle_menu(cx);
                    if let Err(error) = space_themes::set(&space, theme.as_deref(), cx) {
                        eprintln!("mooring: {error}");
                    }
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        rows
    }
    pub(super) fn space_tiles(&self, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        use crate::ui::theme::Flat as F;
        let mut out: Vec<gpui::AnyElement> = vec![];
        // The spaces as chips that wrap: each its colors, its name and how many of its
        // sessions need you. A click narrows the list below to that space (and shows it on
        // the right); a second click lists every space again.
        let filter = self.space_filter().map(str::to_owned);
        let needs = crate::model::session_state::needs_by_space(&self.workspace);
        let mut chips = div()
            .flex()
            .flex_wrap()
            .gap(px(F::CHIP_GAP))
            .px(px(T::S12))
            .pt(px(T::S10))
            .pb(px(T::S2))
            .font_family(T::UI_FONT);
        for space in &self.workspace.spaces {
            let id = space.id.clone();
            if let Some((_, field, _)) = self
                .layout
                .renaming_space
                .as_ref()
                .filter(|(s, ..)| s.id == id)
            {
                // Named in place: the chip widens into a field until Enter.
                chips = chips.child(
                    div()
                        .w_full()
                        .h(px(T::ICON_HEIGHT + T::S4))
                        .px(px(T::S8 - T::BORDER_WIDTH))
                        .flex()
                        .items_center()
                        .gap(px(T::S6))
                        .rounded(px(T::CONTROL_RADIUS))
                        .bg(T::color(T::raised()))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::focus()))
                        .text_size(px(T::SMALL))
                        .child(self.space_dot(&space.id, T::S10))
                        .child(div().flex_1().min_w_0().h_full().child(field.clone())),
                );
                continue;
            }
            let on = filter.as_deref() == Some(id.as_str());
            let dimmed = filter.is_some() && !on;
            let missing = space_dir_missing(space);
            let need = needs.get(id.as_str()).copied().unwrap_or(0);
            let fix = id.clone();
            chips = chips.child(
                div()
                    .id(gpui::SharedString::from(format!("space-{id}")))
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(if missing {
                        format!("{}，目录不在了，点一下重新选择", space.name)
                    } else if on {
                        format!("{}，正在只看这个空间，点一下看全部", space.name)
                    } else {
                        format!("只看 {}", space.name)
                    })
                    .h(px(F::CHIP))
                    .max_w_full()
                    .pl(px(T::S6))
                    .pr(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(F::CHIP_GAP))
                    .rounded(px(F::CHIP / 2.))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(if on {
                        F::chip_on_line()
                    } else {
                        F::chip_line()
                    }))
                    .when(on, |d| d.bg(T::color(F::chip_on())))
                    .when(dimmed, |d| d.opacity(0.5))
                    .cursor_pointer()
                    .hover(|s| s.border_color(T::color(F::chip_on_line())).opacity(1.))
                    .child(self.space_dot(&space.id, F::CHIP_DOT))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(F::CHIP_TEXT))
                            .line_height(px(F::LINE_SMALL))
                            .text_color(if missing {
                                Tone::Danger.ink()
                            } else if on {
                                T::color(F::bright())
                            } else {
                                T::color(F::quiet())
                            })
                            .child(space.name.clone()),
                    )
                    .when(need > 0, |d| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .min_w(px(F::BADGE))
                                .h(px(F::BADGE))
                                .px(px(T::S4))
                                .rounded(px(F::BADGE / 2.))
                                .bg(Status::Waiting.colors().0)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(F::BADGE_TEXT))
                                .line_height(px(F::BADGE))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(T::color(T::WHITE))
                                .child(need.to_string()),
                        )
                    })
                    .tooltip(super::diagnostics::tip(vec![if missing {
                        "目录不在了，点一下重新选择".to_owned()
                    } else {
                        format!("{} · {}", space.name, space.cwd)
                    }]))
                    .on_mouse_down(gpui::MouseButton::Right, {
                        let id = id.clone();
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                            this.layout.space_menu = Some((id.clone(), event.position));
                            this.layout.space_switch = false;
                            this.layout.space_removing = false;
                            cx.stop_propagation();
                            cx.notify();
                        })
                    })
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if missing {
                                this.choose_space_dir(Some(fix.clone()), window, cx);
                                return;
                            }
                            if event.click_count() == 2 {
                                this.rename_space(&id, window, cx);
                                return;
                            }
                            this.toggle_space_filter(&id, cx);
                        }),
                    ),
            );
        }
        // A new space is the last chip: a dashed ring with a plus.
        chips = chips.child(
            div()
                .id("add-space")
                .role(gpui::accesskit::Role::Button)
                .aria_label("新建空间")
                .size(px(F::CHIP))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border(px(T::BORDER_WIDTH))
                .border_dashed()
                .border_color(T::color(F::chip_on_line()))
                .cursor_pointer()
                .hover(|s| s.bg(T::color(F::chip_on())))
                .text_size(px(T::SMALL))
                .text_color(T::color(F::third()))
                .child("+")
                .on_click(cx.listener(|this, _, window, cx| this.new_space(window, cx))),
        );
        out.push(chips.into_any_element());
        // Narrowed to one space: what it is, its folder's branch, and the way back.
        if let Some(space) = filter
            .as_deref()
            .and_then(|f| self.workspace.spaces.iter().find(|s| s.id == f))
        {
            let branch = self.git_branch(&space.cwd, cx);
            let counts = self
                .git_divergence(&space.cwd, cx)
                .and_then(super::divergence::divergence_label);
            out.push(
                div()
                    .mt(px(T::S10))
                    .mx(px(T::S12))
                    .px(px(T::S10))
                    .py(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .rounded(px(T::S10))
                    .bg(T::color(F::scope()))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(F::chip_line()))
                    .text_size(px(T::SMALL))
                    .line_height(px(F::LINE_TEXT))
                    .child(self.space_dot(&space.id, T::S12))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(space.name.clone()),
                    )
                    .when_some(branch, |d, branch| {
                        d.child(
                            label(branch)
                                .min_w_0()
                                .truncate()
                                .text_size(px(T::CHIP_SIZE))
                                .text_color(T::color(T::muted())),
                        )
                    })
                    .when_some(counts, |d, counts| {
                        d.child(
                            label(counts)
                                .flex_shrink_0()
                                .text_size(px(T::CHIP_SIZE))
                                .text_color(T::color(T::muted()))
                                .font_features(T::tabular()),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        button("space-filter-clear", "✕")
                            .aria_label("看全部空间")
                            .flex_shrink_0()
                            .h(px(T::ICON_HEIGHT))
                            .px(px(T::S6))
                            .border_0()
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar.space_filter = None;
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }
        out
    }
}
