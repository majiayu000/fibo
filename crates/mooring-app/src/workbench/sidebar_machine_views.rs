//! The three layouts around other machines' rows, as the preset says: machine chips over
//! one mixed list, a rail that picks one machine, or a folding section per machine. The
//! column keeps its width in all three, so nothing right of it moves.
use super::sidebar_flat::fold_mark;
use super::sidebar_list::{recent_order, split_today};
use super::sidebar_machines::{RemoteRow, machine_label, machine_letter, machine_mark};
use super::*;
use crate::machines::Link;
use crate::ui::theme::Flat as F;
use mooring_core::sidebar::{Bucket, Machines};

fn need_badge(need: usize) -> gpui::Div {
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
        .child(need.to_string())
}

/// A folding section's heading.
struct Heading {
    key: String,
    mark: Option<gpui::Div>,
    name: String,
    /// How it stands: its session count, or why it is not connected.
    note: String,
    need: usize,
    open: bool,
    /// The machine `＋` opens a terminal on.
    add: Option<usize>,
}

impl Workbench {
    /// What the list opens with: the space chips, and what the layout adds to them.
    pub(super) fn column_top(&self, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        match (self.machine_layout(), self.machine_scope()) {
            (Machines::Rail, Some(index)) => {
                vec![self.remote_space_chips(index).into_any_element()]
            }
            (Machines::Sections, _) => {
                let open = self.sidebar.open("machine:local", true);
                let mut out = vec![self.local_section_heading(open, cx)];
                if open {
                    out.extend(self.space_tiles(cx));
                }
                out
            }
            (Machines::Mixed, _) => {
                let mut out = self.space_tiles(cx);
                out.push(self.machine_chips(cx).into_any_element());
                out
            }
            _ => self.space_tiles(cx),
        }
    }
    /// A chip per other machine under the space chips: a click lists only that machine,
    /// a second click lists every one again.
    fn machine_chips(&self, cx: &Context<Self>) -> gpui::Div {
        let scope = self.machine_scope();
        let chips = self.machines.iter().enumerate().map(|(index, remote)| {
            let (host, online) = {
                let view = remote.view.lock().unwrap();
                (view.spec.host.clone(), view.link == Link::Online)
            };
            let name = machine_label(&host).to_owned();
            let need = self.remote_needs(index);
            let on = scope == Some(index);
            div()
                .id(gpui::SharedString::from(format!("machine-chip-{index}")))
                .role(gpui::accesskit::Role::Button)
                .aria_label(if on {
                    format!("正在只看 {name}，点一下看全部")
                } else {
                    format!("只看 {name}")
                })
                .h(px(F::CHIP))
                .max_w_full()
                .pl(px(T::S4))
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
                .cursor_pointer()
                .hover(|s| s.border_color(T::color(F::chip_on_line())))
                .child(machine_mark(machine_letter(&name), !online))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(F::CHIP_TEXT))
                        .line_height(px(F::LINE_SMALL))
                        .text_color(T::color(if !online {
                            F::third()
                        } else if on {
                            F::bright()
                        } else {
                            F::quiet()
                        }))
                        .child(if online {
                            name.clone()
                        } else {
                            format!("{name} · 断开")
                        }),
                )
                .when(need > 0, |d| d.child(need_badge(need)))
                .tooltip(super::diagnostics::tip(vec![
                    match self.machine_limit(index) {
                        Some(limit) => format!("{name} · {limit}"),
                        None => name.clone(),
                    },
                ]))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let again = this.sidebar.machine_scope.as_deref() == Some(host.as_str());
                    this.sidebar.machine_scope = (!again).then(|| host.clone());
                    this.sidebar.space_filter = None;
                    cx.notify();
                }))
        });
        div()
            .mx(px(T::S12))
            .mt(px(T::S8))
            .pt(px(T::S8))
            .flex()
            .flex_wrap()
            .gap(px(F::CHIP_GAP))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(F::line()))
            .font_family(T::UI_FONT)
            .children(chips)
    }
    /// That machine's spaces, in the chips' look; they only name what is there.
    fn remote_space_chips(&self, index: usize) -> gpui::Div {
        let rows = self.remote_rows_of(index);
        let spaces = self
            .machines
            .get(index)
            .map_or_else(Vec::new, |m| m.view.lock().unwrap().spaces.clone());
        let chips = spaces.into_iter().map(|space| {
            let need = rows
                .iter()
                .filter(|r| r.online && r.card.needs_you && r.card.space == space.id)
                .count();
            div()
                .h(px(F::CHIP))
                .max_w_full()
                .pl(px(T::S6))
                .pr(px(T::S8))
                .flex()
                .items_center()
                .gap(px(F::CHIP_GAP))
                .rounded(px(F::CHIP / 2.))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(F::chip_line()))
                .child(self.space_dot(&space.id, F::CHIP_DOT))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(F::CHIP_TEXT))
                        .line_height(px(F::LINE_SMALL))
                        .text_color(T::color(F::quiet()))
                        .child(space.name),
                )
                .when(need > 0, |d| d.child(need_badge(need)))
        });
        div()
            .flex()
            .flex_wrap()
            .gap(px(F::CHIP_GAP))
            .px(px(T::S12))
            .pt(px(T::S10))
            .pb(px(T::S2))
            .font_family(T::UI_FONT)
            .children(chips)
    }

    fn section_heading(&self, h: Heading, cx: &Context<Self>) -> gpui::AnyElement {
        const HEADING: f32 = 34.;
        let Heading {
            key,
            mark,
            name,
            note,
            need,
            open,
            add,
        } = h;
        let toggle = key.clone();
        div()
            .id(gpui::SharedString::from(format!("section-{key}")))
            .role(gpui::accesskit::Role::Button)
            .aria_label(format!(
                "{name}，{}",
                if open {
                    "点一下收起"
                } else {
                    "点一下展开"
                }
            ))
            .mt(px(T::S10))
            .h(px(HEADING))
            .flex_shrink_0()
            .pl(px(T::S14))
            .pr(px(T::S10))
            .flex()
            .items_center()
            .gap(px(T::S6))
            .border_t(px(T::BORDER_WIDTH))
            .border_b(px(T::BORDER_WIDTH))
            .border_color(T::color(F::line()))
            .bg(T::color(F::hover()))
            .cursor_pointer()
            .font_family(T::UI_FONT)
            .child(fold_mark(open))
            .children(mark)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(T::SMALL))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(F::bright()))
                    .child(name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(F::TAG))
                    .text_color(T::color(F::third()))
                    .child(note),
            )
            .when(need > 0, |d| d.child(need_badge(need)))
            .when_some(add, |d, index| {
                d.child(
                    div()
                        .id(gpui::SharedString::from(format!("machine-open-{index}")))
                        .role(gpui::accesskit::Role::Button)
                        .aria_label(format!("在 {name} 上开一个新终端"))
                        .size(px(T::S16 + T::S4))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(T::S4))
                        .text_color(T::color(F::second()))
                        .hover(|s| s.bg(T::color(F::selected())))
                        .child("+")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_remote_shell(index, window, cx);
                        })),
                )
            })
            .on_click(
                cx.listener(move |this, _, _, cx| this.toggle_sidebar_group(&toggle, open, cx)),
            )
            .into_any_element()
    }
    fn local_section_heading(&self, open: bool, cx: &Context<Self>) -> gpui::AnyElement {
        let sessions = self
            .workspace
            .agent_sessions
            .iter()
            .filter(|s| self.session_hidden(s).is_none() && self.session_open(s))
            .count();
        self.section_heading(
            Heading {
                key: "machine:local".into(),
                mark: None,
                name: "本机".into(),
                note: format!("{sessions} 个会话"),
                need: self.needs_you().len(),
                open,
                add: None,
            },
            cx,
        )
    }
    /// One other machine's section: its heading, then its rows in 会话优先's order. Folded
    /// by default while it is not connected.
    fn machine_section(&self, index: usize, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let Some(host) = self.machine_host(index) else {
            return Vec::new();
        };
        let link = self.machines[index].view.lock().unwrap().link.clone();
        let rows = self.remote_rows_of(index);
        let key = Self::machine_section_key(&host);
        let online = link == Link::Online;
        let open = self.sidebar.open(&key, online);
        let name = machine_label(&host).to_owned();
        let note = match &link {
            Link::Online => match self.machine_limit(index) {
                Some(limit) => format!("{} 个会话 · {limit}", rows.len()),
                None => format!("{} 个会话", rows.len()),
            },
            Link::Connecting => "连接中…".to_owned(),
            Link::Offline(why) => format!("断开：{why}"),
        };
        let mut out = vec![self.section_heading(
            Heading {
                key: key.clone(),
                mark: Some(machine_mark(machine_letter(&name), !online)),
                name,
                note,
                need: self.remote_needs(index),
                open,
                add: (online && self.machine_manages(index)).then_some(index),
            },
            cx,
        )];
        if !open {
            return out;
        }
        let keyed: Vec<(&RemoteRow, Bucket, u64)> =
            rows.iter().map(|r| (r, r.bucket, r.when)).collect();
        let order = recent_order(&keyed);
        let history = order
            .stopped
            .into_iter()
            .chain(order.quiet)
            .map(|r| (r, r.when))
            .collect();
        let (today, older) = split_today(history, crate::model::time::now());
        for (part, title, rows) in [
            ("need", mooring_core::status::NEEDS_YOU, order.need),
            ("running", Status::Running.label(), order.running),
            ("today", "今天", today),
            ("older", "更早", older),
        ] {
            let part = format!("{key}:{part}");
            if !rows.is_empty() {
                out.push(self.flat_fold(&part, title, rows.len(), cx));
                let open = self.sidebar.open(&part, true);
                for r in rows.into_iter().filter(|r| open || r.shown) {
                    out.push(self.remote_row(r, cx));
                }
            }
        }
        out
    }
    /// What follows this machine's list.
    pub(super) fn after_local_list(
        &self,
        recent: bool,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        match self.machine_layout() {
            Machines::Below => self.machine_rows(recent, cx),
            Machines::Sections => (0..self.machines.len())
                .flat_map(|i| self.machine_section(i, cx))
                .collect(),
            Machines::Mixed | Machines::Rail => Vec::new(),
        }
    }

    /// The rail's button for this machine (`None`) or another.
    fn rail_button(&self, index: Option<usize>, cx: &Context<Self>) -> gpui::AnyElement {
        const BUTTON: f32 = 32.;
        let view = index.and_then(|i| Some(self.machines.get(i)?.view.lock().unwrap().clone()));
        let online = view.as_ref().is_none_or(|v| v.link == Link::Online);
        let host = view.as_ref().map(|v| v.spec.host.clone());
        let name = host.as_deref().map_or("本机", machine_label).to_owned();
        let on = self.machine_scope() == index;
        let need = index.map_or_else(|| self.needs_you().len(), |i| self.remote_needs(i));
        let face = match index {
            None => gpui::svg()
                .path("icons/terminal.svg")
                .size(px(F::ICON))
                .text_color(T::color(if on { F::bright() } else { F::second() }))
                .into_any_element(),
            Some(_) => div()
                .text_size(px(T::SMALL))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(T::color(if !online {
                    F::third()
                } else if on {
                    F::bright()
                } else {
                    F::quiet()
                }))
                .child(machine_letter(&name))
                .into_any_element(),
        };
        let label = if online {
            name.clone()
        } else {
            format!("{name}，断开")
        };
        div()
            .id(gpui::SharedString::from(format!(
                "rail-{}",
                index.map_or_else(|| "local".to_owned(), |i| i.to_string())
            )))
            .role(gpui::accesskit::Role::Button)
            .aria_label(label.clone())
            .relative()
            .size(px(BUTTON))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(T::S8))
            .border(px(T::BORDER_WIDTH))
            .when(!online, |d| d.border_dashed())
            .border_color(T::color(if on {
                F::chip_on_line()
            } else {
                F::chip_line()
            }))
            .when(on, |d| d.bg(T::color(F::selected())))
            .cursor_pointer()
            .hover(|s| s.bg(T::color(F::hover())))
            .child(face)
            .when(need > 0, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(-T::S4))
                        .right(px(-T::S4))
                        .child(need_badge(need)),
                )
            })
            .tooltip(super::diagnostics::tip(vec![
                match index.and_then(|i| self.machine_limit(i)) {
                    Some(limit) => format!("{label} · {limit}"),
                    None => label,
                },
            ]))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.sidebar.machine_scope = host.clone();
                this.sidebar.space_filter = None;
                cx.notify();
            }))
            .into_any_element()
    }
    fn machine_rail(&self, cx: &Context<Self>) -> gpui::Div {
        const RAIL: f32 = 44.;
        div()
            .w(px(RAIL))
            .flex_shrink_0()
            .pt(px(T::S6))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(T::S10))
            .border_r(px(T::BORDER_WIDTH))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(F::line()))
            .child(self.rail_button(None, cx))
            .children((0..self.machines.len()).map(|i| self.rail_button(Some(i), cx)))
    }
    /// 会话优先's column at its usual width, with the rail inside it when the layout has one.
    pub(super) fn flat_column(
        &self,
        list: gpui::Stateful<gpui::Div>,
        listed: usize,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let rail = (self.machine_layout() == Machines::Rail).then(|| self.machine_rail(cx));
        div()
            .id("sidebar")
            .w(px(F::WIDTH))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(T::color(F::bg()))
            .border_r(px(T::BORDER_WIDTH))
            .border_color(T::color(F::line()))
            .child(self.flat_lights(cx))
            .child(
                div().flex_1().min_h_0().flex().children(rail).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(self.flat_brand())
                        .child(self.flat_search(cx))
                        .child(list)
                        .child(self.flat_keys(cx))
                        .child(self.flat_foot(listed, cx)),
                ),
            )
    }
    /// What the brand row names: this machine, or the one shown alone, and whether it is up.
    pub(super) fn column_machine(&self) -> (String, bool) {
        self.machine_scope()
            .filter(|_| self.machine_layout() == Machines::Rail)
            .and_then(|i| {
                let view = self.machines.get(i)?.view.lock().unwrap();
                let name = machine_label(&view.spec.host).to_owned();
                Some(if view.link == Link::Online {
                    (name, true)
                } else {
                    (format!("{name} · 断开"), false)
                })
            })
            .unwrap_or_else(|| ("本机".to_owned(), true))
    }
    /// The foot's first words.
    pub(super) fn column_foot(&self, sessions: usize) -> String {
        match self
            .machine_scope()
            .filter(|_| self.machine_layout() == Machines::Rail)
        {
            Some(i) => format!(
                "{} · {} 个会话",
                self.column_machine().0,
                self.remote_rows_of(i).len()
            ),
            None => format!(
                "本机 · {} 个空间 · {sessions} 个会话",
                self.workspace.spaces.len()
            ),
        }
    }
}
