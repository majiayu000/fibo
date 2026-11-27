//! A machine's spaces under its heading, and a new one made there from this window
//! (docs/multi-machine/spec.md): the folders come from its host's `dirs`, the space from
//! its `addspace`, so a host too old for them simply offers no ＋.
use super::*;
use crate::ui::field::{Field, FieldEvent};
use crate::ui::theme::Flat as F;
use fibo_remote::protocol::{Dirs, Op, Space};
use std::time::Duration;

const REPLY: Duration = Duration::from_secs(15);
/// Typing settles for this long before its host is asked again.
const SETTLE: Duration = Duration::from_millis(150);
const ROW: f32 = 24.;
const SHOWN: usize = 5;
const OFFERED: usize = 30;

mod edit_space;

/// The panel under a machine's chips while a new space is being chosen.
pub(in crate::workbench) struct AddingSpace {
    host: String,
    field: Entity<Field>,
    _watch: [gpui::Subscription; 2],
    text: String,
    offered: Vec<String>,
    home: String,
    sending: bool,
    error: Option<String>,
    /// Which question the next reply must answer: a later one makes it stale.
    asked: u64,
    /// Folders there are listed, and the space made, over this short exchange.
    brief: crate::machines::Brief,
    /// The space there this panel renames or sets the folder of; none makes a new one.
    editing: Option<Space>,
}

fn is_path(text: &str) -> bool {
    text.starts_with('/') || text.starts_with('~')
}
/// A path as typed splits into the folder to list and what to look for in it.
fn split_path(text: &str) -> (&str, &str) {
    match text.rfind('/') {
        Some(0) => ("/", &text[1..]),
        Some(at) => (&text[..at], &text[at + 1..]),
        None => (text, ""),
    }
}
/// What to ask the host for what is typed.
fn listing_op(text: &str) -> Op {
    let (path, query) = if text.is_empty() {
        (None, None)
    } else if is_path(text) {
        (Some(split_path(text).0.to_owned()), None)
    } else {
        (None, Some(text.to_owned()))
    };
    Op::Dirs { path, query }
}
/// The folders to offer from its reply: what terminals use first when nothing is typed.
fn offered(reply: &Dirs, text: &str) -> Vec<String> {
    let leaf = is_path(text).then(|| split_path(text).1.to_lowercase());
    let found = (reply.dirs.iter())
        .filter(|d| (leaf.as_deref()).is_none_or(|leaf| d.name.to_lowercase().contains(leaf)))
        .map(|d| d.path.clone());
    let recent = reply.recent.iter().filter(|_| text.is_empty()).cloned();
    let mut seen = std::collections::HashSet::new();
    recent
        .chain(found)
        .filter(|p| seen.insert(p.clone()))
        .take(OFFERED)
        .collect()
}
/// What a new space there is made from, or what an edited one changes to.
#[derive(Debug, PartialEq, Clone)]
enum Make {
    Folder(String),
    /// A folder to make there first, then as `Folder`.
    NewFolder(String),
    /// A space of this name with no folder, as this machine's 新建空间 makes.
    Named(String),
}
/// What Enter means: a path as written; a name is the new space's own where its host takes
/// one (`spacename`), else (an older host) its first folder.
fn target(text: &str, offered: &[String], names: bool) -> Result<Make, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        Err(t!("先选或写一个目录"))
    } else if is_path(text) {
        Ok(Make::Folder(text.to_owned()))
    } else if names {
        Ok(Make::Named(text.to_owned()))
    } else {
        (offered.first().cloned().map(Make::Folder)).ok_or(t!("没有找到这个目录"))
    }
}
fn shorter(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => path.to_owned(),
    }
}
fn same_folder(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

impl Workbench {
    fn machine_index(&self, host: &str) -> Option<usize> {
        (0..self.machines.len()).find(|i| self.machine_host(*i).as_deref() == Some(host))
    }
    /// Whether its host is connected now, so folders there can be picked.
    fn machine_adds_spaces(&self, index: usize) -> bool {
        (self.machines.get(index)).is_some_and(|m| m.view.lock().unwrap().link == Link::Online)
    }
    /// The space this machine's rows are narrowed to; one that has since gone narrows
    /// nothing.
    pub(in crate::workbench) fn machine_space_of(&self, index: usize) -> Option<Space> {
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
        self.open_space_panel(index, None, window, cx);
    }
    fn open_space_panel(
        &mut self,
        index: usize,
        editing: Option<Space>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.machine_host(index) else {
            return;
        };
        let text = editing.as_ref().map(|s| s.name.clone()).unwrap_or_default();
        let field = cx.new(|cx| {
            Field::new(t!("目录"), &text, cx)
                .inline()
                .placeholder(t!("~/code/项目，或名字"))
        });
        let events = cx.subscribe(&field, |this, _, event, cx| match event {
            FieldEvent::Submit => this.submit_machine_space(None, cx),
            FieldEvent::Cancel => this.cancel_machine_space(cx),
            FieldEvent::ClickOut => {}
        });
        // Fields notify for a blinking cursor too: only a changed text asks the host again.
        let changes = cx.observe(&field, |this, field, cx| {
            let text = field.read(cx).text.trim().to_owned();
            let Some(adding) = &mut this.layout.adding_machine_space else {
                return;
            };
            if adding.text != text {
                adding.text = text;
                adding.error = None;
                this.list_machine_dirs(true, cx);
                cx.notify();
            }
        });
        window.focus(&field.focus_handle(cx), cx);
        let spec = (self.machine_index(&host))
            .and_then(|i| self.machines.get(i))
            .map(|m| m.view.lock().unwrap().spec.clone())
            .unwrap_or_else(|| crate::machines::Spec::new(host.clone()));
        self.layout.adding_machine_space = Some(AddingSpace {
            brief: crate::machines::Brief::new(spec),
            host,
            field,
            _watch: [events, changes],
            text,
            offered: Vec::new(),
            home: String::new(),
            sending: false,
            error: None,
            asked: 0,
            editing,
        });
        self.list_machine_dirs(false, cx);
        cx.notify();
    }
    pub(in crate::workbench) fn cancel_machine_space(&mut self, cx: &mut Context<Self>) {
        if self.layout.adding_machine_space.take().is_some() {
            self.nav.pending_focus = true;
            cx.notify();
        }
    }
    fn adding_link(&self) -> Option<crate::machines::Brief> {
        Some(self.layout.adding_machine_space.as_ref()?.brief.clone())
    }
    fn list_machine_dirs(&mut self, settle: bool, cx: &mut Context<Self>) {
        let Some(linked) = self.adding_link() else {
            return;
        };
        let Some(adding) = &mut self.layout.adding_machine_space else {
            return;
        };
        adding.asked += 1;
        let (asked, text) = (adding.asked, adding.text.clone());
        let op = listing_op(&text);
        let timer = cx.background_executor().clone();
        let sent = cx.background_spawn(async move {
            if settle {
                timer.timer(SETTLE).await;
            }
            linked.request(op, REPLY)
        });
        cx.spawn(async move |this, cx| {
            let reply = sent.await;
            let _ = this.update(cx, |this, cx| {
                let Some(adding) = &mut this.layout.adding_machine_space else {
                    return;
                };
                if adding.asked != asked {
                    return;
                }
                // A folder that is not there lists nothing, and Enter says why.
                let dirs = reply
                    .ok()
                    .and_then(|v| serde_json::from_value::<Dirs>(v).ok());
                adding.offered = dirs.as_ref().map_or_else(Vec::new, |d| offered(d, &text));
                if let Some(dirs) = dirs {
                    adding.home = dirs.home;
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// Makes the space there: `chosen`, or what Enter means. It stays open with the reason
    /// when refused; once made, the column lists that space. A space that exists already
    /// comes back the same way.
    fn submit_machine_space(&mut self, chosen: Option<Make>, cx: &mut Context<Self>) {
        let Some(linked) = self.adding_link() else {
            return;
        };
        let Some(adding) = &mut self.layout.adding_machine_space else {
            return;
        };
        if adding.sending {
            return;
        }
        let names = adding.brief.lists("spacename") || adding.editing.is_some();
        let make = chosen.map_or_else(|| target(&adding.text, &adding.offered, names), Ok);
        let make = make.and_then(|make| edit_space::allowed(adding, make));
        let make = match make {
            Ok(make) => make,
            Err(why) => {
                adding.error = Some(why.to_owned());
                cx.notify();
                return;
            }
        };
        // A name that already is a space there picks that one rather than a second of it.
        if let (Make::Named(name), None) = (&make, &adding.editing) {
            let host = adding.host.clone();
            if let Some(space) = self.machine_space_named(&host, name) {
                self.sidebar.machine_space = Some(space.id);
                self.layout.adding_machine_space = None;
                self.nav.pending_focus = true;
                cx.notify();
                return;
            }
        }
        let Some(adding) = &mut self.layout.adding_machine_space else {
            return;
        };
        adding.sending = true;
        adding.error = None;
        adding.asked += 1;
        let (asked, host) = (adding.asked, adding.host.clone());
        let editing = adding.editing.as_ref().map(|s| s.id.clone());
        let sent = cx.background_spawn(async move {
            let make = match make {
                // Made there first; the space takes the folder its host says it made.
                Make::NewFolder(path) => {
                    let request = Some(fibo_core::protocol::RequestId::fresh().id);
                    let made = linked.request(Op::Mkdir { path, request }, REPLY)?;
                    let path = made["path"].as_str().unwrap_or_default().to_owned();
                    Make::Folder(path)
                }
                make => make,
            };
            linked.request(edit_space::space_op(make, editing), REPLY)
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let reply = sent.await;
            let _ = this.update(cx, |this, cx| {
                this.machine_space_made(&host, asked, reply, cx)
            });
        })
        .detach();
    }
    fn machine_space_named(&self, host: &str, name: &str) -> Option<Space> {
        let remote = self.machines.get(self.machine_index(host)?)?;
        let view = remote.view.lock().unwrap();
        view.spaces.iter().find(|s| s.name == name).cloned()
    }
    fn machine_space_made(
        &mut self,
        host: &str,
        asked: u64,
        reply: std::io::Result<serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        let made = reply.and_then(|v| serde_json::from_value::<Space>(v).map_err(Into::into));
        // Cancelled while it was on its way: it is there all the same, and the next list
        // shows it. Nothing to pick and nowhere to say.
        let current = (self.layout.adding_machine_space.as_ref())
            .is_some_and(|a| a.asked == asked && a.host == host);
        if !current {
            return;
        }
        match made {
            Ok(space) => {
                // The next list says so too; until then the chip is already there, or
                // already reads as changed.
                if let Some(remote) = self.machine_index(host).and_then(|i| self.machines.get(i)) {
                    let mut view = remote.view.lock().unwrap();
                    match view.spaces.iter_mut().find(|s| s.id == space.id) {
                        Some(known) => *known = space.clone(),
                        None => view.spaces.push(space.clone()),
                    }
                }
                self.sidebar.machine_space = Some(space.id);
                self.layout.adding_machine_space = None;
                self.nav.pending_focus = true;
            }
            Err(e) => {
                if let Some(adding) = &mut self.layout.adding_machine_space {
                    adding.sending = false;
                    adding.error = Some(e.to_string());
                }
            }
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
        let spaces = self.machines[index].view.lock().unwrap().spaces.clone();
        let rows = adding.offered.iter().map(|path| {
            let known = spaces.iter().any(|s| same_folder(&s.cwd, path));
            let chosen = path.clone();
            div()
                .id(gpui::SharedString::from(format!("machine-dir-{path}")))
                .role(gpui::accesskit::Role::Button)
                .aria_label(if known {
                    t!("{path}，已是空间", path)
                } else {
                    path.clone()
                })
                .h(px(ROW))
                .flex_shrink_0()
                .px(px(T::S6))
                .flex()
                .items_center()
                .gap(px(T::S8))
                .rounded(px(T::S4))
                .cursor_pointer()
                .hover(|s| s.bg(T::color(F::selected())))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(T::mono(cx).family)
                        .text_size(px(F::TIME))
                        .text_color(T::color(F::title()))
                        .child(shorter(path, &adding.home)),
                )
                .when(known, |d| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(F::TAG))
                            .text_color(T::color(F::third()))
                            .child(t!("已是空间")),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.submit_machine_space(Some(Make::Folder(chosen.clone())), cx);
                }))
        });
        let enter = self.enter_row(adding, &spaces, cx);
        let shown = adding.offered.len().min(SHOWN);
        let words = if adding.sending {
            Some((t!("正在 {name} 上建…", name), T::color(F::third())))
        } else {
            adding.error.clone().map(|e| (e, Tone::Danger.colors().1))
        };
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
                    .child(match &adding.editing {
                        Some(space) => t!("改 {name} 上的「{space}」", name, space = space.name),
                        None => t!("在 {name} 上新建空间", name),
                    }),
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
            .when_some(enter.filter(|_| !adding.sending), |d, row| d.child(row))
            .when(!adding.sending && shown > 0, |d| {
                // A definite height: docs/performance on wrappers sized by max_h.
                d.child(
                    div()
                        .id("machine-dirs")
                        .h(px(ROW * shown as f32))
                        .flex_shrink_0()
                        .overflow_y_scroll()
                        .children(rows),
                )
            })
            .when_some(words, |d, (words, ink)| {
                d.child(div().text_size(px(F::TAG)).text_color(ink).child(words))
            })
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
    fn empty_machine_space(&self, index: usize, space: Space, cx: &Context<Self>) -> gpui::Div {
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
    pub fn visual_submit_machine_space(&mut self, cx: &mut Context<Self>) {
        self.submit_machine_space(None, cx);
    }
    /// Right-click on that machine's space `id`.
    pub fn visual_begin_edit_machine_space(
        &mut self,
        index: usize,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let space = self.machines[index]
            .view
            .lock()
            .unwrap()
            .spaces
            .iter()
            .find(|s| s.id == id)
            .cloned();
        self.begin_edit_machine_space(index, space.expect("no such space"), window, cx);
    }
    /// The words of the row above the folders, and a click on it.
    pub fn visual_machine_space_choice(
        &mut self,
        click: bool,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let adding = self.layout.adding_machine_space.as_ref()?;
        let host = adding.host.clone();
        let spaces = (self.machine_index(&host).and_then(|i| self.machines.get(i)))
            .map_or_else(Vec::new, |m| m.view.lock().unwrap().spaces.clone());
        let (words, _, make) = edit_space::visual_choice(adding, &spaces)?;
        if click {
            self.submit_machine_space(Some(make), cx);
        }
        Some(words)
    }
    pub fn visual_cancel_machine_space(&mut self, cx: &mut Context<Self>) {
        self.cancel_machine_space(cx);
    }
    /// Whether the panel is open, what it offers and what it says.
    pub fn visual_adding_machine_space(&self) -> Option<(Vec<String>, Option<String>, bool)> {
        (self.layout.adding_machine_space.as_ref())
            .map(|a| (a.offered.clone(), a.error.clone(), a.sending))
    }
    pub fn visual_machine_space_chosen(&self) -> Option<String> {
        self.sidebar.machine_space.clone()
    }
    pub fn visual_machine_adds_spaces(&self, index: usize) -> bool {
        self.machine_adds_spaces(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibo_remote::protocol::Dir;

    fn dir(path: &str) -> Dir {
        Dir {
            name: path.rsplit('/').next().unwrap().to_owned(),
            path: path.to_owned(),
            git: None,
            sessions: 0,
            recent: false,
        }
    }
    fn reply(dirs: &[&str], recent: &[&str]) -> Dirs {
        Dirs {
            path: String::new(),
            parent: None,
            home: "/home/me".to_owned(),
            dirs: dirs.iter().map(|d| dir(d)).collect(),
            recent: recent.iter().map(|r| (*r).to_owned()).collect(),
            truncated: false,
        }
    }

    #[test]
    fn what_is_typed_decides_what_the_host_is_asked() {
        let asks = |text| match listing_op(text) {
            Op::Dirs { path, query } => (path, query),
            other => panic!("{other:?}"),
        };
        assert_eq!(asks(""), (None, None));
        assert_eq!(asks("vsr"), (None, Some("vsr".to_owned())));
        assert_eq!(asks("~/code/v"), (Some("~/code".to_owned()), None));
        assert_eq!(asks("~/code/"), (Some("~/code".to_owned()), None));
        assert_eq!(asks("/opt"), (Some("/".to_owned()), None));
        assert_eq!(asks("~"), (Some("~".to_owned()), None));
    }
    #[test]
    fn nothing_typed_offers_what_terminals_use_first_and_each_folder_once() {
        let r = reply(&["/home/me/a", "/home/me/b"], &["/home/me/b", "/srv/c"]);
        assert_eq!(offered(&r, ""), ["/home/me/b", "/srv/c", "/home/me/a"]);
    }
    #[test]
    fn a_typed_path_narrows_its_folder_by_the_last_part() {
        let r = reply(&["/code/vsr", "/code/Vault", "/code/x"], &["/recent"]);
        assert_eq!(offered(&r, "~/code/V"), ["/code/vsr", "/code/Vault"]);
        assert_eq!(offered(&r, "~/code/").len(), 3);
    }
    #[test]
    fn enter_takes_a_path_as_written_a_name_as_its_first_match_and_says_why_not() {
        let found = ["/code/vsr".to_owned()];
        let folder = |p: &str| Ok(Make::Folder(p.to_owned()));
        assert_eq!(target("~/nope", &found, false), folder("~/nope"));
        assert_eq!(target("vsr", &found, false), folder("/code/vsr"));
        assert_eq!(target("  ", &found, false), Err("先选或写一个目录"));
        assert_eq!(target("vsr", &[], false), Err("没有找到这个目录"));
    }
    #[test]
    fn a_host_that_takes_names_makes_the_space_typed_not_a_folder_found() {
        let found = ["/code/vsr-old".to_owned()];
        let named = Ok(Make::Named("vsr".to_owned()));
        assert_eq!(target(" vsr ", &found, true), named);
        assert_eq!(target("vsr", &[], true), named);
        assert_eq!(
            target("~/code/vsr", &found, true),
            Ok(Make::Folder("~/code/vsr".into()))
        );
        assert_eq!(target("", &found, true), Err("先选或写一个目录"));
    }
    #[test]
    fn home_shortens_only_as_a_whole_folder() {
        assert_eq!(shorter("/home/me/a", "/home/me"), "~/a");
        assert_eq!(shorter("/home/me", "/home/me"), "~");
        assert_eq!(shorter("/home/melody", "/home/me"), "/home/melody");
        assert_eq!(shorter("/a", ""), "/a");
    }
    #[test]
    fn a_trailing_slash_is_the_same_folder() {
        assert!(same_folder("/a/b/", "/a/b"));
        assert!(!same_folder("/a/b", "/a/bc"));
    }
}
