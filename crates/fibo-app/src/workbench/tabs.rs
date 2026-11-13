//! Terminal tabs: their shown name, renaming and order.
use super::*;

/// A tab being dragged along the strip, drawn under the pointer by its name.
#[derive(Clone)]
pub(super) struct DraggedTab {
    pub(super) id: String,
    pub(super) title: String,
}
impl gpui::Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        drag_chip(self.title.clone())
    }
}
/// What a dragged tab or pane is drawn as under the pointer: its name on a chip.
pub(super) fn drag_chip(title: String) -> gpui::Div {
    div()
        .h(px(T::TABS))
        .max_w(px(T::TAB_MAX_WIDTH))
        .px(px(T::S12))
        .flex()
        .items_center()
        .rounded(px(T::S4))
        .bg(T::color(T::raised()))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(T::border()))
        .text_size(px(T::MONO_SIZE))
        .text_color(T::color(T::primary()))
        .opacity(0.9)
        .child(div().truncate().child(title))
}

impl Workbench {
    /// The tabs of a tab's own space, in strip order.
    pub(super) fn sibling_tabs(&self, id: &str) -> Vec<String> {
        let Some(space) = self
            .workspace
            .tabs
            .iter()
            .find(|t| t.id == id)
            .map(|t| &t.space)
        else {
            return vec![];
        };
        self.workspace
            .tabs
            .iter()
            .filter(|t| &t.space == space)
            .map(|t| t.id.clone())
            .collect()
    }
    /// One step left (-1) or right (1) among the tabs of its space.
    pub(super) fn step_tab(&mut self, id: &str, step: i32, cx: &mut Context<Self>) {
        let tabs = self.sibling_tabs(id);
        let Some(at) = tabs.iter().position(|t| t == id) else {
            return;
        };
        let edit = match step {
            -1 if at > 0 => Edit::MoveTab {
                id: id.into(),
                before: Some(tabs[at - 1].clone()),
            },
            1 if at + 1 < tabs.len() => Edit::MoveTab {
                id: tabs[at + 1].clone(),
                before: Some(id.into()),
            },
            _ => return,
        };
        self.edit(edit, cx);
    }
    /// Program titles come from the daemon, which reads every terminal, so tabs in
    /// the back keep theirs. The focused pane's title wins over its siblings'.
    pub(super) fn tab_name(&self, tab: &Tab, _cx: &gpui::App) -> String {
        super::redraw::tab_name(
            &self.workspace,
            tab,
            &self.store.recognition().terminal_titles,
            self.nav.focused.as_deref(),
            now(),
        )
    }
    /// Double-click names a tab; an empty name hands it back to its contents.
    pub(super) fn rename_tab(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.save_tab_name(cx);
        let Some(tab) = self.workspace.tabs.iter().find(|t| t.id == id) else {
            return;
        };
        // A tab never named starts empty: its automatic name, often a shortened
        // first ask, is shown in grey instead of being handed over to edit.
        let current = tab.title.clone();
        let automatic = self.tab_name(
            &Tab {
                title: String::new(),
                ..tab.clone()
            },
            cx,
        );
        let field = cx.new(|cx| {
            Field::new(t!("标签名"), &current, cx)
                .inline()
                .ends_on_click_out()
                .placeholder(&automatic)
        });
        field.update(cx, |f, _| f.select_all());
        let events = cx.subscribe(&field, |this, _, event, cx| match event {
            // These hand the keyboard back to the terminal; a click on something that
            // takes the keyboard blurs the field instead and leaves focus where it landed.
            crate::ui::field::FieldEvent::Submit | crate::ui::field::FieldEvent::ClickOut => {
                this.save_tab_name(cx);
                this.nav.pending_focus = true;
            }
            crate::ui::field::FieldEvent::Cancel => {
                this.layout.renaming_tab = None;
                this.nav.pending_focus = true;
                cx.notify();
            }
        });
        // Clicking anywhere else keeps the name, as in Finder.
        let blur = cx.on_blur(&field.focus_handle(cx), window, |this, _, cx| {
            this.save_tab_name(cx)
        });
        window.focus(&field.focus_handle(cx), cx);
        self.layout.renaming_tab = Some((id.to_owned(), field, [events, blur]));
        cx.notify();
    }
    fn save_tab_name(&mut self, cx: &mut Context<Self>) {
        let Some((id, field, _)) = self.layout.renaming_tab.take() else {
            return;
        };
        let Some(tab) = self.workspace.tabs.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        let name = field.read(cx).text.trim().to_owned();
        // A tab holding one conversation is named by naming the conversation: the name
        // has one owner, which the sidebar, notifications and the phone all read.
        let sole = self
            .workspace
            .agent_sessions
            .iter()
            .find(|s| {
                s.current
                    && self
                        .workspace
                        .sole_session_tab(s)
                        .is_some_and(|t| t.id == tab.id)
            })
            .map(|s| {
                (
                    s.id.clone(),
                    s.name.clone(),
                    self.workspace.session_title(s),
                )
            });
        if let Some((session, given, shown)) = sole {
            // Keeping the shown name unchanged must not pin today's automatic one.
            let name = if given.is_empty() && name == shown {
                String::new()
            } else {
                name
            };
            if name != given {
                self.edit(Edit::RenameSession { id: session, name }, cx);
            }
            cx.notify();
            return;
        }
        // Keeping the shown name unchanged must not pin today's automatic title.
        let title = if tab.title.is_empty() && name == self.tab_name(&tab, cx) {
            String::new()
        } else {
            name
        };
        self.edit(Edit::RenameTab { id: tab.id, title }, cx);
        cx.notify();
    }
}
