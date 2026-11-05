//! 设置 › 快捷键: recording a new chord in place, and the page of every shortcut.
use super::*;

impl SettingsView {
    pub(in crate::workbench) fn record(&mut self, action: Action, cx: &mut Context<Self>) {
        self.page = Page::Keys;
        self.recording = Some(action);
        self.taken = None;
        self.refused = None;
        cx.notify();
    }
    pub(super) fn key(&mut self, k: &gpui::Keystroke, cx: &mut Context<Self>) {
        let Some(action) = self.recording else {
            match keymap::action(k, cx) {
                _ if k.key == "escape" => cx.emit(SettingsAction::Close),
                Some(Action::Close | Action::Settings) => cx.emit(SettingsAction::Close),
                _ => (),
            }
            return;
        };
        if k.key == "escape" {
            self.recording = None;
            cx.notify();
            return;
        }
        // A modifier on its own is the start of a chord, not one.
        let Some(chord) = Chord::from_keystroke(k) else {
            return;
        };
        self.recording = None;
        let mut map = keymap::current(cx);
        if let Some(why) = chord.refusal() {
            self.refused = Some((action, why));
        } else if let Some(holder) = map.holder(action, &chord) {
            self.taken = Some((action, chord, holder));
        } else {
            map.assign(action, chord);
            keymap::update(map, cx);
        }
        cx.notify();
    }
    /// Gives the taken chord to the action that asked for it.
    pub(in crate::workbench) fn take(&mut self, cx: &mut Context<Self>) {
        if let Some((action, chord, _)) = self.taken.take() {
            let mut map = keymap::current(cx);
            map.assign(action, chord);
            keymap::update(map, cx);
        }
        cx.notify();
    }
    pub(in crate::workbench) fn reset(&mut self, action: Option<Action>, cx: &mut Context<Self>) {
        let map = match action {
            Some(action) => {
                let mut map = keymap::current(cx);
                map.reset(action);
                map
            }
            None => Keymap::default(),
        };
        self.taken = None;
        self.refused = None;
        keymap::update(map, cx);
        cx.notify();
    }
    fn key_row(&self, first: bool, action: Action, map: &Keymap, cx: &Context<Self>) -> gpui::Div {
        let name = action.name();
        let recording = self.recording == Some(action);
        let refused = self
            .refused
            .filter(|(a, _)| *a == action)
            .map(|(_, why)| why);
        let taken = self.taken.as_ref().filter(|(a, ..)| *a == action);
        // The row being recorded, or answering for its chord, stands out with its notes.
        let active = recording || refused.is_some() || taken.is_some();
        let chords = map.chords(action);
        let note = |text: String, color: gpui::Hsla| {
            div()
                .flex_1()
                .text_size(px(T::META))
                .text_color(color)
                .child(text)
        };
        let mut caps = div().flex().gap(px(T::S4)).justify_end();
        if recording {
            caps = caps.child(keycap("按下组合键…", true).border_color(T::color(T::focus())));
        } else if chords.is_empty() {
            caps = caps.child(keycap("未设置", true));
        } else {
            for chord in chords {
                caps = caps.child(keycap(chord.display(), false));
            }
        }
        let mut row = div()
            .flex()
            .flex_col()
            .px(px(T::S16))
            .when(!first, |d| {
                d.border_t(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
            })
            .when(active, |d| d.bg(T::color(T::raised())))
            .child(
                div()
                    .h(px(T::TODO_HEIGHT + T::S4))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::primary()))
                            .child(action.label()),
                    )
                    // A fixed slot: the chord never moves when 恢复 comes and goes.
                    .child(div().w(px(RESET_WIDTH)).flex().justify_end().when(
                        !map.is_default(action),
                        |d| {
                            d.child(secondary(format!("reset-{name}"), "恢复").on_click(
                                cx.listener(move |this, _, _, cx| this.reset(Some(action), cx)),
                            ))
                        },
                    ))
                    .child(
                        div()
                            .id(format!("key-{name}"))
                            .role(gpui::accesskit::Role::Button)
                            .aria_label(action.label())
                            .min_w(px(KEY_WIDTH))
                            .flex()
                            .justify_end()
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.75))
                            .child(caps)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.record(action, cx);
                            })),
                    ),
            );
        if recording {
            row = row.child(div().pb(px(T::S10)).flex().child(note(
                "按下新的组合键，需包含 ⌘；Esc 取消".into(),
                T::color(T::secondary()),
            )));
        }
        if let Some(why) = refused {
            row = row.child(
                div()
                    .pb(px(T::S10))
                    .flex()
                    .child(note(why.into(), Tone::Danger.ink())),
            );
        }
        if let Some((_, chord, holder)) = taken {
            row = row.child(
                div()
                    .pb(px(T::S10))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child(note(
                        format!("{} 已被「{}」使用", chord.display(), holder.label()),
                        Tone::Warn.ink(),
                    ))
                    .child(
                        button("take-chord", "仍然使用")
                            .h(px(SEGMENT_HEIGHT))
                            .bg(T::color(T::accent()))
                            .text_color(T::color(T::on_accent()))
                            .on_click(cx.listener(|this, _, _, cx| this.take(cx))),
                    )
                    .child(secondary("keep-chord", "取消").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.taken = None;
                            cx.notify();
                        },
                    ))),
            );
        }
        row
    }
    /// ⌘1–⌘9 are fixed: one read-only row instead of nine.
    fn tab_numbers_row(&self, first: bool) -> gpui::Div {
        div()
            .h(px(T::TODO_HEIGHT + T::S4))
            .px(px(T::S16))
            .flex()
            .items_center()
            .gap(px(T::S8))
            .when(!first, |d| {
                d.border_t(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
            })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_baseline()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::primary()))
                            .child("切换到第几个标签"),
                    )
                    .child(label("⌘9 为最后一个；固定")),
            )
            .child(keycap("⌘1 – ⌘9", true))
    }
    pub(super) fn keys_page(&self, cx: &Context<Self>) -> gpui::Div {
        let map = keymap::current(cx);
        let mut groups = Vec::new();
        for section in Section::ALL {
            let mut rows = Vec::new();
            for action in Keymap::actions().filter(|a| {
                a.section() == section && self.matches(Page::Keys, &a.label(), section.label(), cx)
            }) {
                rows.push(
                    self.key_row(rows.is_empty(), action, &map, cx)
                        .into_any_element(),
                );
            }
            if section == Section::Tabs && self.matches(Page::Keys, "切换到第几个标签", "⌘1 ⌘9", cx)
            {
                rows.push(self.tab_numbers_row(rows.is_empty()).into_any_element());
            }
            groups.push((section.label(), rows));
        }
        self.cards(groups).child(
            div().pt(px(T::S12)).flex().justify_end().child(
                secondary("settings-reset-all", "全部恢复默认")
                    .on_click(cx.listener(|this, _, _, cx| this.reset(None, cx))),
            ),
        )
    }
}
