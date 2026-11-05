//! A space's right-click menu.
use super::*;

impl Workbench {
    /// A space's right-click menu.
    pub(in crate::workbench) fn space_menu_view(
        &self,
        id: &str,
        position: gpui::Point<gpui::Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let rename = id.to_owned();
        let folder = id.to_owned();
        let remove = id.to_owned();
        let tabs = self.workspace.tabs.iter().filter(|t| t.space == id).count();
        let last = self.workspace.spaces.len() <= 1;
        // From the dense presets' space line: every space to switch to, and a new one.
        let mut picks = vec![];
        if self.layout.space_switch {
            for space in &self.workspace.spaces {
                let pick = space.id.clone();
                let SpaceCounts {
                    count,
                    waiting,
                    errors,
                    group,
                } = self.space_counts(&space.id);
                picks.push(
                    button(
                        gpui::SharedString::from(format!("space-pick-{}", space.id)),
                        "",
                    )
                    .aria_label(space.name.clone())
                    .w_full()
                    .px(px(T::S8))
                    .gap(px(T::S8))
                    .justify_start()
                    .border_0()
                    .when(self.space_active(&space.id), |d| {
                        d.bg(T::color(T::button_hover()))
                    })
                    .child(dot(group))
                    .child(div().min_w_0().truncate().child(space.name.clone()))
                    .when(space_dir_missing(space), |d| {
                        d.child(
                            label("目录不在了")
                                .flex_shrink_0()
                                .text_size(px(T::CHIP_SIZE))
                                .text_color(T::color(T::muted())),
                        )
                    })
                    .child(div().flex_1())
                    .when(waiting > 0, |d| d.child(badge(Status::Waiting, waiting)))
                    .when(errors > 0, |d| d.child(badge(Status::Error, errors)))
                    .child(
                        label(count.to_string())
                            .flex_shrink_0()
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(T::color(T::muted())),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout.space_menu = None;
                        this.pick_space(&pick, cx);
                    }))
                    .into_any_element(),
                );
            }
            picks.push(
                button("space-menu-add", "＋ 新空间")
                    .w_full()
                    .justify_start()
                    .border_0()
                    .text_color(T::color(T::link()))
                    .on_click(cx.listener(|this, _, window, cx| this.new_space(window, cx)))
                    .into_any_element(),
            );
        }
        let name = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let has_dir = self
            .workspace
            .spaces
            .iter()
            .any(|s| s.id == id && !s.cwd.is_empty());
        let mut entries = picks;
        entries.extend([
            button(
                "space-menu-rename",
                if self.layout.space_switch {
                    format!("重命名「{name}」")
                } else {
                    "重命名".to_owned()
                },
            )
            .w_full()
            .justify_start()
            .border_0()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.layout.space_menu = None;
                this.rename_space(&rename, window, cx);
            }))
            .into_any_element(),
            button(
                "space-menu-folder",
                if has_dir {
                    "更换目录…"
                } else {
                    "设置目录…"
                },
            )
            .w_full()
            .justify_start()
            .border_0()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.choose_space_dir(Some(folder.clone()), window, cx);
            }))
            .into_any_element(),
            // Two presses: the first says what goes with it, the second removes it.
            button(
                "space-menu-remove",
                if !self.layout.space_removing {
                    "删除空间".to_owned()
                } else if tabs > 0 {
                    format!("确认删除，并关掉 {tabs} 个标签")
                } else {
                    "确认删除".to_owned()
                },
            )
            .w_full()
            .justify_start()
            .border_0()
            .text_color(Tone::Danger.ink())
            .when(last, |d| d.opacity(0.4))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !last {
                    this.remove_space(&remove, cx);
                }
            }))
            .into_any_element(),
        ]);
        entries.extend(self.space_theme_rows(id, cx));
        self.popup(position, entries, window, cx, |this| {
            this.layout.space_menu = None
        })
    }
}
