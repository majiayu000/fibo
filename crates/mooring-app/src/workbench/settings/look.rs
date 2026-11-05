//! 设置 › 外观: light, dark or following macOS, one theme for each side, and the rest of
//! how the window looks.
use super::*;
use crate::ui::appearance::{self, Appearance, Mode};
use crate::ui::palette::Palette;
use crate::ui::themes::{self, Entry, Themes};

const TILE_PREVIEW: f32 = 84.;

/// A small window in one palette: a column a step off the lit pane, as the real one has it.
fn window_sketch(p: &Palette) -> gpui::Div {
    let (bg, fg) = (p.term.bg, p.term.fg);
    let line = |share: u32, width: f32| {
        div()
            .h(px(T::S4))
            .w(px(width))
            .rounded_full()
            .bg(T::color(mix(bg, fg, share)))
    };
    div()
        .flex_1()
        .h_full()
        .flex()
        .bg(T::color(p.sidebar_bg))
        .child(
            div()
                .w(px(T::S20 + T::S4))
                .h_full()
                .p(px(T::S6))
                .flex()
                .flex_col()
                .gap(px(T::S4))
                .child(line(400, T::S12))
                .child(line(250, T::S10))
                .child(line(250, T::S12)),
        )
        .child(
            div()
                .flex_1()
                .m(px(T::S4))
                .ml_0()
                .p(px(T::S8))
                .flex()
                .flex_col()
                .gap(px(T::S6))
                .rounded(px(T::S2))
                .bg(T::color(bg))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(p.border))
                .child(line(650, T::S20 * 2.))
                .child(line(300, T::S20 * 3.))
                .child(line(300, T::S20 * 2.5)),
        )
}
const PER_ROW: usize = 3;
impl SettingsView {
    fn tile(
        &self,
        id: impl Into<gpui::ElementId>,
        name: impl Into<gpui::SharedString>,
        on: bool,
        picture: impl IntoElement,
    ) -> gpui::Stateful<gpui::Div> {
        let name = name.into();
        div()
            .id(id)
            .role(gpui::accesskit::Role::RadioButton)
            .aria_label(name.clone())
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(T::S8))
            .cursor_pointer()
            .child(
                div()
                    .w_full()
                    .h(px(TILE_PREVIEW))
                    .flex()
                    .overflow_hidden()
                    .rounded(px(T::CONTROL_RADIUS + T::S2))
                    .border(px(T::FOCUS_RING))
                    .border_color(T::color(if on { T::accent() } else { T::border() }))
                    .child(picture),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::SMALL))
                    .text_color(T::color(if on { T::primary() } else { T::secondary() }))
                    .when(on, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                    .child(
                        div()
                            .size(px(T::DOT + T::S4))
                            .rounded_full()
                            .border(px(if on { T::S4 } else { T::BORDER_WIDTH }))
                            .border_color(T::color(if on { T::accent() } else { T::control() })),
                    )
                    .child(name),
            )
    }
    pub(super) fn look_page(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = self.theme_rows(cx);
        let mut style = Vec::new();
        if self.shows(Page::Look, "界面样式", cx) {
            use crate::ui::style::{self, Style};
            let chosen = style::current(cx);
            let mut choices = segments();
            for (i, look) in Style::ALL.into_iter().enumerate() {
                choices = choices.child(
                    segment(format!("style-{i}"), look.label(), look == chosen).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.text_error = style::choose(look, cx).err();
                            cx.notify();
                        }),
                    ),
                );
            }
            style.push(
                self.row(
                    true,
                    "界面样式",
                    "标准：现在的布局去掉重复；终端原生：外壳退成细线，等宽字，标签在底部",
                    choices,
                )
                .into_any_element(),
            );
        }
        let mut icon = Vec::new();
        if self.shows(Page::Look, "应用图标", cx) {
            let current = cx.global::<Chosen>().icon;
            let mut tiles = div().flex().gap(px(T::S16)).p(px(T::S16));
            for choice in AppIcon::ALL {
                let picture = div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(T::color(T::raised()))
                    .child(gpui::img(choice.title_image()).size(px(ICON_PREVIEW)));
                tiles = tiles.child(
                    self.tile(
                        format!("icon-{}", choice.label()),
                        choice.label(),
                        choice == current,
                        picture,
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        layout::choose_icon(choice, cx);
                        cx.refresh_windows();
                        cx.notify();
                    })),
                );
            }
            icon.push(tiles.into_any_element());
            icon.push(
                div()
                    .px(px(T::S16))
                    .pb(px(T::S12))
                    .child(label("标题栏和程序坞一起更换，下次启动仍保留"))
                    .into_any_element(),
            );
        }
        let mut motion = Vec::new();
        if self.shows(Page::Look, "标记完成动画", cx) {
            use mooring_core::effect;
            let note = if effect::reduce_motion() {
                "系统开了「减弱动态效果」，现在不管选哪种都只淡出"
            } else {
                "在 插件 › 完成动画 里试播、换一个或装新的；试播在当前窗格播一次，不会标记完成"
            };
            let now = effect::picked_label(effect::picked().as_ref(), effect::presets());
            motion.push(
                self.row(
                    true,
                    "标记完成动画",
                    note,
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        .child(
                            div()
                                .text_size(px(T::SMALL))
                                .text_color(T::color(T::primary()))
                                .child(now),
                        )
                        .child(secondary("complete-preview", "试播").on_click(cx.listener(
                            |_, _, _, cx| cx.emit(SettingsAction::PreviewComplete(None)),
                        )))
                        .child(secondary("complete-change", "换一个").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.show_page(Page::Plugins, cx);
                            },
                        ))),
                )
                .into_any_element(),
            );
        }
        if self.shows(Page::Look, "状态圆圈转动", cx) {
            use crate::ui::running_spin;
            let on = running_spin::current(cx);
            let note = if mooring_core::effect::reduce_motion() {
                "系统开了「减弱动态效果」，现在不会转".to_owned()
            } else {
                format!(
                    "开启后，有「{}」的会话时窗口每秒多画约 10 次，CPU 约多占 3–5% 个核，也更耗电；窗口被完全挡住时不转",
                    Status::Running.label()
                )
            };
            let toggle = switch("running-spin", on).on_click(cx.listener(move |this, _, _, cx| {
                this.text_error = running_spin::choose(!on, cx).err();
                cx.refresh_windows();
                cx.notify();
            }));
            motion.push(
                self.row(motion.is_empty(), "状态圆圈转动", &note, toggle)
                    .into_any_element(),
            );
        }
        let mut asks = Vec::new();
        if self.shows(Page::Look, "钉住这一轮的提问", cx) {
            use crate::ui::pinned_ask::{self, PinnedAsk};
            let chosen = pinned_ask::current(cx);
            type Way = fn(&mut PinnedAsk) -> &mut bool;
            let ways: [(&str, &str, Way); 3] = [
                ("pinned-ask-bar", "终端顶部钉一条", |c| &mut c.bar),
                ("pinned-ask-float", "往回翻时浮出", |c| &mut c.float),
                ("pinned-ask-button", "标题栏「↑ 你问」", |c| {
                    &mut c.button
                }),
            ];
            let mut choices = segments();
            for (id, name, field) in ways {
                let mut on = chosen;
                let selected = *field(&mut on);
                choices = choices.child(segment(id, name, selected).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let mut next = pinned_ask::current(cx);
                        let f = field(&mut next);
                        *f = !*f;
                        this.text_error = pinned_ask::choose(next, cx).err();
                        cx.refresh_windows();
                        cx.notify();
                    },
                )));
            }
            let note = if !chosen.any() {
                "都没选：回答再长也不钉提问"
            } else if chosen.bar && chosen.button {
                "可多选；顶部条和标题栏按钮同时开时只显示顶部条。自己会钉提问的 Agent（Codex）不画"
            } else {
                "可多选；提问还在屏幕上时不画，自己会钉提问的 Agent（Codex）也不画"
            };
            asks.push(
                self.row(true, "钉住这一轮的提问", note, choices)
                    .into_any_element(),
            );
        }
        self.cards(vec![
            ("主题", theme),
            ("界面样式", style),
            ("应用图标", icon),
            ("动效", motion),
            ("对话", asks),
        ])
    }
    /// 明暗 on one row, then each side's themes. Light and dark show only their own side,
    /// so a tile always means what clicking it will show.
    fn theme_rows(&self, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let mut rows = Vec::new();
        if !self.shows(Page::Look, "主题", cx) {
            return rows;
        }
        let chosen = cx.try_global::<Appearance>().cloned().unwrap_or_default();
        let Some(themes) = cx.try_global::<Themes>() else {
            return rows;
        };
        let mut modes = segments();
        for (i, mode) in Mode::ALL.into_iter().enumerate() {
            modes = modes.child(
                segment(format!("mode-{i}"), mode.label(), mode == chosen.mode).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.text_error = appearance::set_mode(mode, cx).err();
                        crate::set_menus(cx);
                        cx.notify();
                    }),
                ),
            );
        }
        let terminal = crate::ui::style::current(cx) == crate::ui::style::Style::Terminal;
        let note = if chosen.mode != Mode::System {
            "窗口与终端一起切换"
        } else if terminal {
            "终端原生样式下跟随系统按暗色显示"
        } else {
            "macOS 变换时，在下面两组之间切换"
        };
        rows.push(self.row(true, "明暗", note, modes).into_any_element());
        if let Some(shown) = appearance::shown(cx)
            && shown.source == appearance::Source::Space
            && let Some(entry) = shown.entry
        {
            rows.push(
                self.row(
                    false,
                    &format!("当前空间单独用着「{}」", entry.name),
                    "下面选的主题在其他空间生效；右键空间也能换",
                    secondary("space-theme-release", "让这个空间跟随外观").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.text_error = crate::ui::space_themes::release_current(cx).err();
                            cx.notify();
                        },
                    )),
                )
                .into_any_element(),
            );
        }
        let shows_light = chosen.shows_light(cx.window_appearance(), crate::ui::style::current(cx));
        let sides: &[bool] = match chosen.mode {
            Mode::System => &[true, false],
            Mode::Light => &[true],
            Mode::Dark => &[false],
        };
        for &light in sides {
            let title = if light {
                "亮色主题"
            } else {
                "暗色主题"
            };
            let on = chosen.entry(themes, light).map(|e| e.id.clone());
            let in_use = chosen.mode == Mode::System && shows_light == light;
            rows.push(
                self.theme_side(title, in_use, themes, light, on.as_deref(), cx)
                    .into_any_element(),
            );
        }
        rows.push(self.theme_actions(themes, cx).into_any_element());
        rows
    }
    fn theme_side(
        &self,
        title: &'static str,
        in_use: bool,
        themes: &Themes,
        light: bool,
        on: Option<&str>,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let entries: Vec<&Entry> = themes.side(light).collect();
        let mut grid = div().flex().flex_col().gap(px(T::S16));
        for row in entries.chunks(PER_ROW) {
            let mut line = div().flex().gap(px(T::S16));
            for entry in row {
                let id = entry.id.clone();
                let picked = on == Some(entry.id.as_str());
                line = line.child(
                    self.tile(
                        format!("theme-{}", entry.id),
                        entry.name.clone(),
                        picked,
                        div().flex_1().flex().child(window_sketch(&entry.palette)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.text_error = appearance::set_theme(&id, light, false, cx).err();
                        crate::set_menus(cx);
                        cx.notify();
                    })),
                );
            }
            for _ in row.len()..PER_ROW {
                line = line.child(div().flex_1());
            }
            grid = grid.child(line);
        }
        div()
            .px(px(T::S16))
            .pt(px(T::S12))
            .pb(px(T::S4))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .pb(px(T::S12))
                    .child(
                        div()
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::primary()))
                            .child(title),
                    )
                    .when(in_use, |d| d.child(label("正在用"))),
            )
            .child(grid)
    }
    /// 调整, 导入 and the folder, under the tiles; then anything a file got wrong.
    fn theme_actions(&self, themes: &Themes, cx: &Context<Self>) -> gpui::Div {
        let problems = themes
            .problems
            .iter()
            .fold(div().flex().flex_col(), |d, (file, why)| {
                d.child(
                    div()
                        .text_size(px(T::SMALL))
                        .text_color(crate::ui::tone::Tone::Warn.ink())
                        .child(format!("{file}：{why}")),
                )
            });
        div()
            .px(px(T::S16))
            .py(px(T::S12))
            .flex()
            .flex_col()
            .gap(px(T::S8))
            .child(
                div()
                    .flex()
                    .gap(px(T::S8))
                    .child(
                        secondary("theme-edit", "调整当前主题")
                            .on_click(cx.listener(|this, _, _, cx| this.open_editor(cx))),
                    )
                    .child(
                        secondary("theme-import", "导入主题或 .itermcolors")
                            .on_click(cx.listener(|this, _, _, cx| this.import_theme(cx))),
                    )
                    .child(
                        secondary("theme-folder", "打开主题文件夹").on_click(cx.listener(
                            |this, _, _, cx| {
                                if let Some(dir) = themes::dir() {
                                    let _ = std::fs::create_dir_all(&dir);
                                    cx.reveal_path(&dir);
                                }
                                this.text_error = None;
                            },
                        )),
                    ),
            )
            .when_some(self.text_error.clone(), |d, error| {
                d.child(
                    div()
                        .text_size(px(T::SMALL))
                        .text_color(crate::ui::tone::Tone::Danger.ink())
                        .child(error),
                )
            })
            .child(problems)
    }
    fn import_theme(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("导入".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.first() else { return };
            let done = themes::import(path).and_then(|file| {
                let light = file.term_colors().light;
                themes::save(&file, None).map(|id| (id, light))
            });
            let _ = this.update(cx, |this, cx| {
                this.text_error = done
                    .and_then(|(id, light)| {
                        themes::reload(cx);
                        appearance::set_theme(&id, light, true, cx)
                    })
                    .err();
                crate::set_menus(cx);
                cx.notify();
            });
        })
        .detach();
    }
}
