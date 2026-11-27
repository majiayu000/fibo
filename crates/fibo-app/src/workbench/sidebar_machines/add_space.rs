//! A machine's spaces under its heading, and a new one made there from this window
//! (docs/multi-machine/spec.md): named as one is here, through that machine's own
//! workbench, its folder chosen later in its empty tab.
use super::*;
use crate::ui::field::{Field, FieldEvent};
use crate::ui::theme::Flat as F;
use fibo_remote::protocol as rp;

/// The panel under a machine's chips while a new space is being named.
pub(in crate::workbench) struct AddingSpace {
    host: String,
    field: Entity<Field>,
    _watch: gpui::Subscription,
    error: Option<String>,
}

/// The space Enter makes from what is typed: that name, no folder yet, as 「＋」 makes one
/// here. What is typed is never looked up as a folder.
fn named_space(text: &str) -> Result<Space, &'static str> {
    match text.trim() {
        "" => Err(t!("先写一个名字")),
        name => Ok(Space {
            id: super::super::id(),
            name: name.to_owned(),
            cwd: String::new(),
        }),
    }
}

impl Workbench {
    fn machine_index(&self, host: &str) -> Option<usize> {
        (0..self.machines.len()).find(|i| self.machine_host(*i).as_deref() == Some(host))
    }
    /// Whether a space can be made there now: connected, through its own workbench here.
    fn machine_adds_spaces(&self, index: usize) -> bool {
        let online =
            (self.machines.get(index)).is_some_and(|m| m.view.lock().unwrap().link == Link::Online);
        online && self.machine_manages(index)
    }
    /// The space this machine's rows are narrowed to; one that has since gone narrows
    /// nothing.
    pub(in crate::workbench) fn machine_space_of(&self, index: usize) -> Option<rp::Space> {
        let id = self.sidebar.machine_space.as_deref()?;
        let view = self.machines.get(index)?.view.lock().unwrap();
        view.spaces.iter().find(|s| s.id == id).cloned()
    }

    pub(in crate::workbench) fn begin_add_machine_space(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(host), Some(view)) = (self.machine_host(index), self.machine_view(index)) else {
            return;
        };
        // 「新空间」, or 「新空间 2」 when that machine has one by that name.
        let name = super::super::sidebar_spaces::new_space_name(&view.read(cx).workspace.spaces);
        let field = cx.new(|cx| {
            let mut f = Field::new(t!("新空间的名字"), &name, cx).inline();
            f.select_all();
            f
        });
        let events = cx.subscribe_in(&field, window, |this, _, event, window, cx| match event {
            FieldEvent::Submit => this.submit_machine_space(window, cx),
            FieldEvent::Cancel => this.cancel_machine_space(cx),
            FieldEvent::ClickOut => {}
        });
        window.focus(&field.focus_handle(cx), cx);
        self.layout.adding_machine_space = Some(AddingSpace {
            host,
            field,
            _watch: events,
            error: None,
        });
        cx.notify();
    }
    pub(in crate::workbench) fn cancel_machine_space(&mut self, cx: &mut Context<Self>) {
        if self.layout.adding_machine_space.take().is_some() {
            self.nav.pending_focus = true;
            cx.notify();
        }
    }
    /// Makes the space there under the typed name, chooses it, and shows it on the right,
    /// where its empty tab offers that machine's recent folders. Stays open saying why when
    /// there is no name or the machine went.
    fn submit_machine_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(adding) = &self.layout.adding_machine_space else {
            return;
        };
        let host = adding.host.clone();
        let made = named_space(&adding.field.read(cx).text);
        let index = self.machine_index(&host);
        let view = index.and_then(|i| self.machine_view(i));
        let space = match (made, index, view) {
            (Ok(space), Some(index), Some(view)) => {
                view.update(cx, |w, cx| w.edit(Edit::AddSpace(space.clone()), cx));
                (space, index)
            }
            (made, ..) => {
                let why = made.err().map_or_else(
                    || t!("{host} 没连上，连上后再建", host = machine_label(&host)).to_owned(),
                    str::to_owned,
                );
                if let Some(adding) = &mut self.layout.adding_machine_space {
                    adding.error = Some(why);
                }
                cx.notify();
                return;
            }
        };
        let (space, index) = space;
        // Its host's next list says so too; until then the chip is already there.
        if let Some(remote) = self.machines.get(index) {
            let mut v = remote.view.lock().unwrap();
            if !v.spaces.iter().any(|s| s.id == space.id) {
                v.spaces.push(rp::Space {
                    id: space.id.clone(),
                    name: space.name.clone(),
                    cwd: String::new(),
                });
            }
        }
        self.layout.adding_machine_space = None;
        self.sidebar.machine_space = Some(space.id.clone());
        self.sidebar.space_filter = None;
        let go = super::super::hosts::Go::Space(space.id);
        if !self.open_on_host(index, go, window, cx) {
            self.nav.pending_focus = true;
        }
        cx.notify();
    }

    /// What follows a machine's heading: its chips, the panel while one is being made, and
    /// what the chosen space holds. With `sections` one space needs no chips, and the
    /// narrowing is said in words as this machine's own list does.
    pub(in crate::workbench) fn machine_spaces(
        &self,
        index: usize,
        sections: bool,
        rows: usize,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let Some(host) = self.machine_host(index) else {
            return Vec::new();
        };
        let spaces = self.machines[index].view.lock().unwrap().spaces.len();
        let can_add = self.machine_adds_spaces(index);
        let mut out = Vec::new();
        if !sections || spaces >= 2 {
            out.push(
                self.remote_space_chips(index, can_add, cx)
                    .into_any_element(),
            );
        } else if can_add {
            out.push(self.first_machine_space(index, cx).into_any_element());
        }
        if let Some(adding) =
            (self.layout.adding_machine_space.as_ref()).filter(|a| can_add && a.host == host)
        {
            out.push(self.add_space_panel(index, adding, cx).into_any_element());
        }
        if let Some(space) = self.machine_space_of(index) {
            if sections {
                out.push(self.machine_scope_line(&space.name, cx).into_any_element());
            }
            if rows == 0 {
                out.push(
                    self.empty_machine_space(index, space, cx)
                        .into_any_element(),
                );
            }
        }
        out
    }
    /// The dashed ＋ among a machine's chips.
    pub(in crate::workbench) fn add_space_chip(
        &self,
        index: usize,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(gpui::SharedString::from(format!(
                "machine-add-space-{index}"
            )))
            .role(gpui::accesskit::Role::Button)
            .aria_label(t!("在这台电脑上新建空间"))
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
            .on_click(cx.listener(move |this, _, window, cx| {
                this.begin_add_machine_space(index, window, cx);
            }))
    }
    /// With one space there are no chips to sit among.
    fn first_machine_space(&self, index: usize, cx: &Context<Self>) -> gpui::Div {
        div()
            .px(px(T::S12))
            .pt(px(T::S10))
            .pb(px(T::S2))
            .flex()
            .items_center()
            .gap(px(F::CHIP_GAP))
            .font_family(T::UI_FONT)
            .child(self.add_space_chip(index, cx))
            .child(
                div()
                    .text_size(px(F::TAG))
                    .text_color(T::color(F::third()))
                    .child(t!("新建空间")),
            )
    }
    fn add_space_panel(&self, index: usize, adding: &AddingSpace, cx: &Context<Self>) -> gpui::Div {
        let name = self.machine_host(index).unwrap_or_default();
        let name = machine_label(&name).to_owned();
        div()
            .mx(px(T::S12))
            .mt(px(T::S6))
            .mb(px(T::S4))
            .p(px(T::S10))
            .flex()
            .flex_col()
            .gap(px(T::S6))
            .rounded(px(T::S8))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(F::line()))
            .bg(T::color(T::raised()))
            .font_family(T::UI_FONT)
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.cancel_machine_space(cx)))
            .child(
                div()
                    .text_size(px(T::SMALL))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(F::bright()))
                    .child(t!("在 {name} 上新建空间", name)),
            )
            .child(
                div()
                    .h(px(T::ROW))
                    .px(px(T::S8 - T::BORDER_WIDTH))
                    .rounded(px(T::CONTROL_RADIUS))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::focus()))
                    .text_size(px(T::SMALL))
                    .child(adding.field.clone()),
            )
            .child(
                div()
                    .text_size(px(F::TAG))
                    .text_color(match adding.error {
                        Some(_) => Tone::Danger.colors().1,
                        None => T::color(F::third()),
                    })
                    .child(adding.error.clone().unwrap_or_else(|| {
                        t!("回车建好，目录可以之后在它的标签页里选").to_owned()
                    })),
            )
    }
    /// 「只看 X」 as this machine's own list says it.
    fn machine_scope_line(&self, name: &str, cx: &Context<Self>) -> gpui::Div {
        div()
            .mt(px(T::S8))
            .mb(px(T::S4))
            .mx(px(F::EDGE))
            .flex()
            .items_center()
            .gap(px(T::S6))
            .font_family(T::UI_FONT)
            .text_size(px(T::SMALL))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(T::color(F::third()))
                    .child(t!("只看")),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(F::bright()))
                    .child(name.to_owned()),
            )
            .child(div().flex_1().min_w(px(T::S8)))
            .child(
                div()
                    .id("machine-space-clear")
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(t!("看全部空间"))
                    .flex_shrink_0()
                    .h(px(T::ICON_HEIGHT))
                    .flex()
                    .items_center()
                    .text_size(px(F::TAG))
                    .text_color(T::color(F::third()))
                    .cursor_pointer()
                    .hover(|s| s.text_color(T::color(F::bright())))
                    .child(t!("看全部"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sidebar.machine_space = None;
                        cx.notify();
                    })),
            )
    }
    fn empty_machine_space(&self, index: usize, space: rp::Space, cx: &Context<Self>) -> gpui::Div {
        div()
            .mx(px(F::EDGE))
            .mt(px(T::S4))
            .mb(px(T::S8))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(T::S6))
            .font_family(T::UI_FONT)
            .text_size(px(F::TAG))
            .text_color(T::color(F::third()))
            .child(t!("这个空间还没有会话。"))
            .when(self.machine_manages(index), |d| {
                d.child(
                    div()
                        .id("machine-space-open")
                        .role(gpui::accesskit::Role::Button)
                        .h(px(F::CHIP))
                        .px(px(T::S8))
                        .flex()
                        .items_center()
                        .rounded(px(T::CONTROL_RADIUS))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(F::chip_line()))
                        .text_color(T::color(F::second()))
                        .cursor_pointer()
                        .hover(|s| s.border_color(T::color(F::chip_on_line())))
                        .child(t!("在这里开一个终端"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_remote_shell(index, Some(space.clone()), window, cx);
                        })),
                )
            })
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    pub fn visual_begin_add_machine_space(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_add_machine_space(index, window, cx);
    }
    /// Types `text` into the panel's own field, as the keyboard would.
    pub fn visual_type_machine_space(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(adding) = &self.layout.adding_machine_space {
            adding.field.update(cx, |f, cx| {
                f.set_text(text);
                cx.notify();
            });
        }
    }
    pub fn visual_submit_machine_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submit_machine_space(window, cx);
    }
    pub fn visual_cancel_machine_space(&mut self, cx: &mut Context<Self>) {
        self.cancel_machine_space(cx);
    }
    /// Whether the panel is open, the name it holds and what it says went wrong.
    pub fn visual_adding_machine_space(&self, cx: &gpui::App) -> Option<(String, Option<String>)> {
        (self.layout.adding_machine_space.as_ref())
            .map(|a| (a.field.read(cx).text.clone(), a.error.clone()))
    }
    pub fn visual_machine_space_chosen(&self) -> Option<String> {
        self.sidebar.machine_space.clone()
    }
    pub fn visual_machine_adds_spaces(&self, index: usize) -> bool {
        self.machine_adds_spaces(index)
    }
    /// Space `id` there, as its host lists it: its name and folder.
    pub fn visual_machine_space_named(&self, index: usize, id: &str) -> Option<(String, String)> {
        let view = self.machines.get(index)?.view.lock().unwrap();
        (view.spaces.iter())
            .find(|s| s.id == id)
            .map(|s| (s.name.clone(), s.cwd.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_typed_is_the_spaces_name_and_no_folder_is_looked_up() {
        let space = named_space("  我的空间 ").unwrap();
        assert_eq!(space.name, "我的空间");
        assert_eq!(space.cwd, "", "a name was taken for a folder");
        assert!(!space.id.is_empty());
        assert_ne!(named_space("x").unwrap().id, space.id);
    }
    #[test]
    fn enter_on_nothing_says_a_name_is_needed() {
        assert_eq!(named_space("   ").unwrap_err(), "先写一个名字");
    }
}
